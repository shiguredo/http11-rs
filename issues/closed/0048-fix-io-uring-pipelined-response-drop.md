# 0048: io_uring サンプルでパイプライン 2 件目以降のレスポンスがドロップされる問題を修正する

Created: 2026-05-12
Completed: 2026-05-12
Model: Opus 4.7

## 概要

`examples/http11_server_io_uring/src/main.rs:638-683` の `ConnectionState::Reading` ブランチで、`while let Some(request) = conn.decoder.decode()?` で複数 Request を順次取り出して関数ローカルの `VecDeque<Response>` に push しているが、`responses.pop_front()` で 1 件のみ取り出して `conn.write_buf` (単一 `Vec<u8>`) に代入し、残り N-1 件は `responses` 変数のスコープ終了で **drop** される。

```rust
// examples/http11_server_io_uring/src/main.rs:638-683
ConnectionState::Reading => {
    let data = conn.read_buf[..bytes_read].to_vec();
    conn.decoder.feed(&data)?;

    let mut responses = VecDeque::new();
    let peer_addr = conn.peer_addr;
    let mut request_count = conn.request_count;

    while let Some(request) = conn.decoder.decode()? {
        request_count += 1;
        let should_keep_alive =
            request.is_keep_alive() && request_count < DEFAULT_MAX_REQUESTS;
        let response = build_response(&request, should_keep_alive)?;
        responses.push_back((response.encode(), should_keep_alive));
    }

    conn.request_count = request_count;

    if let Some((response_bytes, should_keep_alive)) = responses.pop_front() {
        let conn = &mut connections[conn_id];
        conn.write_buf = response_bytes;
        conn.write_offset = 0;
        conn.state = if should_keep_alive {
            ConnectionState::Writing
        } else {
            ConnectionState::Closing
        };
        submit_write(ring, conn_id, fd, connections)?;
    } else {
        submit_read(ring, conn_id, fd, connections)?;
    }
}
// 関数スコープ終了で responses (= 残り N-1 件) が drop される
```

`Connection` 構造体には `pending_writes` / `response_queue` 相当のフィールドが存在しない (`write_buf: Vec<u8>` 単一、`write_offset: usize` 単一)。`handle_write` 完了後は無条件に `submit_read` するため、2 件目以降のレスポンスは **永久にクライアントへ返されない**。

## 根拠

### RFC 9112 §9.3.2 (Pipelining) MUST 違反

```
A client that supports persistent connections MAY "pipeline" its
requests (i.e., send multiple requests without waiting for each
response). A server MAY process a sequence of pipelined requests in
parallel if they all have safe methods (Section 9.2.1 of [HTTP]),
but it MUST send the corresponding responses in the same order
that the requests were received.
```

本実装はパイプライン 1 件目以外のレスポンスを返さないため、`MUST send the corresponding responses in the same order` を満たせない (順序以前に応答そのものが返らない)。

### 発火条件

- HTTP/1.1 Keep-Alive クライアントが 1 TCP segment / 1 read で 2 つ以上のリクエストを送る (パイプライニング)
- kTLS で平文化された後、`handle_read` がまとめて `feed` するため `while let` ループで複数 Request を一度に decode するケースは通常運用で容易に発生する
- 0049 (kTLS 移行時の平文 leftover ドロップ) が先に発火していると 1 件目の境界が壊れ本 issue の症状以前に decode 失敗する → **0049 を先に修正する前提**

### 影響

- パイプライン 2 件目以降のレスポンスがクライアントに届かない
- クライアントは pipelined timeout までコネクションを占有する
- workspace exclude (`Cargo.toml:20`) のため CI で `cargo check` / `cargo clippy` / `cargo test` の対象外、構造的検出不能
- AGENTS.md「サンプルは **お手本** なので性能と堅牢性を両立させること」要件への致命的逸脱

### 関連 issue

- 0049: 同じファイル (`examples/http11_server_io_uring/src/main.rs`) の kTLS 移行時 leftover ドロップ。**0049 を先に修正する前提** (kTLS 移行直後の 1 件目リクエスト境界が壊れていると本 issue の修正効果が見えない)
- 本リポジトリの workspace 復帰 / CI 配線は別 issue で扱う (本 issue のスコープ外)

## スコープ

- `examples/http11_server_io_uring/src/main.rs` の `Connection` 構造体に未送信レスポンスキューを追加し、`handle_write` 完了時に次エントリを書き出すよう変更する
- `should_keep_alive == false` (= `DEFAULT_MAX_REQUESTS` 到達または `Connection: close` 要求) のレスポンスをキューの途中に持つ場合、それ以降のキューエントリは silent drop せず、`should_keep_alive == false` のエントリを書き終えた時点でコネクションを閉じる方針を取る (詳細は対応方針節で記載)
- 含まない:
  - workspace exclude の解除 / CI ジョブ配線 (別 issue)
  - 0049 (kTLS 移行時 leftover) の修正
  - HTTP/1.1 pipelining の本格的並列処理 (RFC 9112 §9.3.2 「MAY process in parallel」の高度実装、`build_response` は順次で十分)

## 対応方針

### `Connection` 構造体の最終形

`write_buf` (進行中の単一書き込みバッファ) と `pending_writes` (未送信キュー) を明確に分離する。io_uring の Write SQE はバッファ実体ポインタの安定性が必要なため、進行中バッファは `Vec<u8>` で固定して `pending_writes` の `VecDeque` 再配置の影響を受けないようにする。

```rust
pub struct Connection {
    // ... 既存フィールド ...
    write_buf: Vec<u8>,                        // 進行中の書き込みバッファ
    write_offset: usize,                        // 進行中バッファ内の進捗
    current_write_should_keep_alive: bool,      // 進行中レスポンスの Keep-Alive フラグ
    pending_writes: VecDeque<(Vec<u8>, bool)>,  // 未送信レスポンスキュー (バイト列, should_keep_alive)
}
```

### `handle_read` (`ConnectionState::Reading`) の改修

`while let` ループで build した全レスポンスを `conn.pending_writes` に push する。ループ中で `should_keep_alive == false` が立ったレスポンスが生まれた時点で、それ以降の Request の decode を停止する (`conn.decoder` の状態は維持して次回読み込みで継続できるようにするか、stale request は無視するかを実装者判断、最も単純なのは「decode を停止して残データは破棄」)。

ループ終了後、`pending_writes` の先頭エントリを `current_write_should_keep_alive` にコピーしつつ `write_buf` に代入して `submit_write` を発行する。キューが空なら `submit_read` を発行する。

### `handle_write` (書き込み完了時) の改修

書き込みが完了したら:

1. `current_write_should_keep_alive == false` ならば `ConnectionState::Closing` に遷移して `close_connection` を発行する。`pending_writes` に残りエントリがあっても破棄する (Connection: close 後に後続レスポンスを送ると RFC 違反)
2. `current_write_should_keep_alive == true` で `pending_writes` が空でなければ次エントリを `pop_front()` で取り出して `current_write_should_keep_alive` を更新、`write_buf` に代入して `submit_write` を発行する
3. `pending_writes` が空ならば `submit_read` を発行する

### `submit_write` 失敗時の復元

`submit_write` は submission queue full で `Err` を返す経路がある (現コード L458-476)。`pop_front` でキューから取り出した後に `submit_write` が失敗した場合、`pending_writes.push_front((write_buf, current_write_should_keep_alive))` で復元してから error を伝播する。

### テスト

io_uring + Linux 環境必須のため CI 自動化は困難。手動再現手順を `examples/http11_server_io_uring/README.md` に追記する。pipelining は `curl` ではデフォルト無効のため、`printf` + `nc` で 1 TCP segment に 2 つのリクエストを詰めて送る方式を採用する:

```sh
printf 'GET /a HTTP/1.1\r\nHost: localhost\r\n\r\nGET /b HTTP/1.1\r\nHost: localhost\r\n\r\n' | nc -q 1 127.0.0.1 8443
```

期待: `/a` と `/b` の両方のレスポンスが順序通りに返ること。

### CHANGES.md

サンプルの挙動修正だが、AGENTS.md「サンプルは **お手本** なので性能と堅牢性を両立」要件に従い `examples/` のバグも本体 `[FIX]` 同等の重要度として扱う。`### misc` ではなく本体 `[FIX]` 配下に記載する (前例: `[FIX] examples ... ` のエントリが本体に並んでいる場合に倣う、現状の CHANGES.md `## develop` に同型エントリがあるか実装時に確認):

```
- [FIX] `examples/http11_server_io_uring` でパイプラインリクエストの 2 件目以降のレスポンスがドロップされる問題を修正する
  - 旧実装は `while let Some(request) = conn.decoder.decode()?` で複数 Request を decode して関数ローカル `VecDeque` に push していたが、`pop_front()` で 1 件目を取り出した後、残り N-1 件はスコープ終了で drop されていた
  - `Connection` 構造体に `pending_writes: VecDeque<(Vec<u8>, bool)>` と `current_write_should_keep_alive: bool` を追加し、`handle_write` 完了時に次エントリを書き出すよう変更する
  - RFC 9112 §9.3.2「a server ... MUST send the corresponding responses in the same order that the requests were received」を満たすよう順序保証する
  - @voluntas
```

### ブランチ

`feature/fix-io-uring-pipelined-response-drop` (`feature/fix-` prefix、サンプル内の構造変更のみで本体 API には影響なし、issue 番号を含まない)。

## 受け入れ基準

- `Connection` 構造体に `pending_writes: VecDeque<(Vec<u8>, bool)>` と `current_write_should_keep_alive: bool` (または等価な状態管理) が追加されている
- `handle_read` で build した全レスポンスが `pending_writes` に push されている
- `handle_write` 完了時に `pending_writes` を `pop_front()` して次の書き込みを発行している
- `current_write_should_keep_alive == false` のレスポンス書き込み完了時に `pending_writes` の残りを破棄して `close_connection` を発行する
- `submit_write` 失敗時に `pending_writes.push_front` で復元してから error を伝播する
- パイプラインで N 件のリクエストを送ったとき、N 件すべてのレスポンスが受信順と一致する順序で返る (手動テスト手順を README に追記)
- `examples/http11_server_io_uring/README.md` に手動テスト手順 (`printf` + `nc` ベース) が追記されている
- CHANGES.md `## develop` に `[FIX]` エントリが追加されている

## RFC 参照

- RFC 9112 §9.3.2 (HTTP/1.1 Pipelining、`MUST send the corresponding responses in the same order`)
- RFC 9112 §9.6 (Connection: close 後の追加メッセージは送るべきでない)

`refs/rfc9112.txt` で参照可能。

## 解決方法

- `examples/http11_server_io_uring/src/main.rs` の `Connection` 構造体に `pending_writes: VecDeque<(Vec<u8>, bool)>` と `current_write_should_keep_alive: bool` を追加した
- `handle_read` の `ConnectionState::Reading` ブランチで、関数ローカル `VecDeque` を撤去し、`conn.pending_writes` に直接 push するよう変更した。`should_keep_alive == false` のレスポンスが出た時点でループを break する
- `handle_read` 末尾の発火経路を `conn.pending_writes.pop_front()` ベースに書き換え、`submit_write` 失敗時は `push_front` で先頭を復元してから error を伝播する
- `handle_write` の `ConnectionState::Writing` 分岐で、書き込み完了時に `pending_writes` から次エントリを取り出して連続送信するよう変更した
- `handle_write` の `ConnectionState::Closing` 分岐で `pending_writes.clear()` を呼び、RFC 9112 Section 9.6 に従って Connection: close 後の追加メッセージを破棄する
- `examples/http11_server_io_uring/README.md` に `openssl s_client` ベースの手動テスト手順を追記した (`nc` は TLS を喋れないため代替手段)
- `CHANGES.md` の `## develop` に `[FIX]` エントリを追加した

io_uring + Linux 環境必須のため CI 自動化は困難で、macOS では `io-uring` クレートが compile しない。ローカル Linux 環境または CI 配線後の検証が必要。
