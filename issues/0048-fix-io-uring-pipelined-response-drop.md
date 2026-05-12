# 0048: io_uring サンプルでパイプライン 2 件目以降のレスポンスがドロップされる問題を修正する

Created: 2026-05-12
Model: Opus 4.7

## 概要

`examples/http11_server_io_uring/src/main.rs:638-683` の `ConnectionState::Reading` ブランチで、`while let Some(request) = conn.decoder.decode()?` で複数 Request を順次取り出して関数ローカルの `VecDeque<Response>` に push しているが、`responses.pop_front()` で 1 件のみ取り出して `conn.write_buf` (単一 `Vec<u8>`) に代入し、残り N-1 件は `responses` 変数のスコープ終了で **drop される**。

```rust
// examples/http11_server_io_uring/src/main.rs:638-683
ConnectionState::Reading => {
    let data = conn.read_buf[..bytes_read].to_vec();
    conn.decoder.feed(&data)?;

    let mut responses = VecDeque::new();      // L643: 関数ローカル
    let peer_addr = conn.peer_addr;
    let mut request_count = conn.request_count;

    while let Some(request) = conn.decoder.decode()? {   // L647: 複数 Request を decode
        request_count += 1;
        let should_keep_alive =
            request.is_keep_alive() && request_count < DEFAULT_MAX_REQUESTS;
        let response = build_response(&request, should_keep_alive)?;
        responses.push_back((response.encode(), should_keep_alive));   // L664: N 件積む
    }

    conn.request_count = request_count;

    if let Some((response_bytes, should_keep_alive)) = responses.pop_front() {   // L669: 1 件のみ
        let conn = &mut connections[conn_id];
        conn.write_buf = response_bytes;          // L671: 単一 Vec に代入
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
// L683: responses (VecDeque) はここでスコープ終了 → 残り N-1 件は drop
```

`Connection` 構造体には `pending_writes` や `response_queue` 相当のフィールドが存在しない (`write_buf` は単一の `Vec<u8>`、`write_offset` は単一進捗カウンタ)。`handle_write` 完了後は無条件に `submit_read` するため、2 件目以降のレスポンスは **永久にクライアントへ返されない**。

## 根拠

### 発火条件

- HTTP/1.1 Keep-Alive クライアントが 1 TCP segment / 1 read で 2 つ以上のリクエストを送る (パイプライニング)
- kTLS で平文化された後、`handle_read` がまとめて `feed` するため `while let` ループで複数 Request を一度に decode するケースは通常運用で容易に発生する
- HTTP/1.1 の標準的な機能 (RFC 9112 §9.3) を踏むだけで決定的に発火

### AGENTS.md との衝突

- 「サンプルは **お手本** なので性能と堅牢性を両立させること」と正面衝突
- workspace exclude (`Cargo.toml:20`) のため CI で `cargo check` / `cargo clippy` / `cargo test` の対象外、検出されない構造的問題も併存

## 影響範囲

- パイプライン 2 件目以降のレスポンスがクライアントに届かない
- クライアントは pipelined timeout までコネクションを占有する
- お手本サンプルとして機能不全

## 対応方針

### `Connection` 構造体の拡張

- `pending_writes: VecDeque<(Vec<u8>, bool)>` を追加する
- `write_buf` は廃止するか、現行を `pending_writes` の先頭エントリのリスナーとして整理する

### `handle_read` (`L638-683`)

- `while let` ループで build した全レスポンスを `conn.pending_writes` に push
- 先頭エントリだけ即時 `submit_write` を発行

### `handle_write` (`L690-757`)

- 1 件書き込み完了後、`conn.pending_writes.pop_front()` で次のエントリを `write_buf` にセットして `submit_write` を発行する
- キューが空になるまで `submit_read` に戻らない
- `should_keep_alive == false` のエントリで `ConnectionState::Closing` に遷移する分岐は維持

### workspace への復帰検討

- `Cargo.toml:20` の `exclude = [...]` から `examples/http11_server_io_uring` を外し、Linux 限定で `#[cfg(target_os = "linux")]` ガードを付ける
- CI に Linux 専用 job を追加し、最低 `cargo check` / `cargo clippy` を通す

### テスト

- 単独 unit test は io_uring 環境必須なので難しいが、`pipelined-with-keep-alive` を curl + http-pipelining 経由で確認するシェルテストを追加する

### CHANGES.md

`## develop` の `### misc` に `[FIX]` として追加する。
