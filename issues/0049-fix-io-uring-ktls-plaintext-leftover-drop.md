# 0049: io_uring サンプルで kTLS 移行時に rustls の復号済み平文を取りこぼす問題を修正する

Created: 2026-05-12
Model: Opus 4.7

## 概要

`examples/http11_server_io_uring/src/main.rs` の TLS ハンドシェイク完了検出経路 (L603-636 と L713-746 の 2 箇所) で、`tls_conn.is_handshaking() == false` を確認した直後に `tls_conn.reader().read(...)` を呼ばずに `dangerous_extract_secrets()` → kTLS 有効化 → `tls_conn.take()` で drop している。TLS 1.3 で `process_new_packets` 時点で rustls が内部復号済みバッファに保持していた Application Data の **先頭バイト** が `tls_conn` の drop と共に失われ、復元不能になる。

`tls_conn.reader().read(...)` の呼び出しは grep 結果 **0 件** で、handshake 完了後の leftover 吸い出し経路が完全に欠落している。

## 根拠

### 発火経路と頻度差

| 経路 | 発火タイミング | leftover 発生頻度 |
|---|---|---|
| L603-636 (`ConnectionState::HandshakeReading` の読み込み完了時) | クライアント Finished + Application Data を含む TCP read 直後 | **高** (curl / openssl s_client / 多くの HTTP/1.1 クライアントが TLS 1.3 で Finished と HTTP リクエストを同一 flight で送るため) |
| L713-746 (`ConnectionState::HandshakeWriting` の書き込み完了時) | サーバ自身の Server Finished 送信完了直後 | **低** (この時点では Client Finished + Application Data はまだ届いていないため `received_plaintext` は通常空) |

L603-636 が主戦場、L713-746 は予防的に同型修正を入れる。

### TLS 1.3 leftover 発生フロー

1. クライアント: ClientHello → 受信 (ServerHello/EncryptedExtensions/Certificate/CertVerify/Finished) → 送信 (Client Finished + 1-RTT で HTTP GET)
2. サーバ受信: 1 回の `read` で Client Finished + Application Data を読む
3. `tls_conn.read_tls` で投入 → `process_new_packets` で復号 → Application Data が rustls 内部の `received_plaintext` バッファに格納
4. `tls_conn.is_handshaking() == false` (Finished 処理済み)
5. 本実装: `tls_conn.take().unwrap()` でローカル変数に move → `dangerous_extract_secrets()` を呼ぶ → ローカル `tls_conn` が関数スコープ終了で drop → **rustls 内部の HTTP リクエスト先頭バイトが消失**
6. `submit_enable_ktls` で kTLS 有効化 → 以降の TCP 新規データはカーネル経由で復号
7. クライアントが既に送信した N バイトはアプリ層から二度と観測できない (TCP レベルでは ACK 済み、再送はない) → タイムアウト / Parse Error / 接続切断

### 影響

- TLS 1.3 + kTLS で HTTP リクエストの先頭バイトが決定的に消失
- 短い HTTP リクエスト (HEAD / GET / 小 POST) ではリクエスト全体が消える可能性
- workspace exclude (`Cargo.toml:20`) のため CI で `cargo check` / `cargo clippy` / `cargo test` の対象外、構造的検出不能
- AGENTS.md「サンプルは **お手本** なので性能と堅牢性を両立させること」要件への致命的逸脱

### 関連 issue

- 0048: 同じファイルのパイプライン 2 件目以降ドロップ。本 issue で drain した leftover に複数 Request が含まれる場合、0048 の `pending_writes` (新設フィールド) を介して順次 build / submit_write する必要がある → **0048 を先に着手して `pending_writes` を導入することが前提**
- workspace 復帰 / CI 配線は別 issue で扱う (本 issue のスコープ外)

## スコープ

- L603-636 経路と L713-746 経路の両方に「`dangerous_extract_secrets()` 前に rustls 内部の復号済み平文を `tls_conn.reader().read_to_end(...)` で吸い出して `conn.decoder.feed(&leftover)` する」処理を追加
- leftover に複数 Request が含まれる場合は 0048 で導入する `pending_writes` に build_response の結果を全件積み、`handle_setsockopt_complete` 完了後に先頭エントリを `submit_write` する
- 含まない:
  - workspace exclude 解除 / CI 配線 (別 issue)
  - 0048 (パイプライン 2 件目以降ドロップ) の修正
  - TLS 1.3 NewSessionTicket / KeyUpdate の取り扱い (本 issue ではスコープ外、別 issue で扱う)

## 対応方針

### `dangerous_extract_secrets()` 前の leftover drain (L603-636)

`tls_conn.take()` 直後、`dangerous_extract_secrets()` 呼び出し前に挿入する:

```rust
let mut tls_conn = conn.tls_conn.take().unwrap();
let mut leftover = Vec::new();
loop {
    match tls_conn.reader().read_to_end(&mut leftover) {
        Ok(_) => break,
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
        Err(e) => {
            // 異常終了は当該接続のみクローズ、サーバプロセスは継続させる
            error!(peer_addr = %peer_addr, error = %e, "Failed to drain plaintext leftover");
            close_connection(ring, conn_id, fd, connections)?;
            return Ok(());
        }
    }
}
let secrets = tls_conn.dangerous_extract_secrets()?;
// ... 既存処理 (ktls_tx / ktls_rx 生成、submit_enable_ktls 発行) ...
```

leftover が非空のとき、`conn.decoder.feed(&leftover)?` で取り込み、その場で `decode()` ループを回して全 Request を build_response し `conn.pending_writes` (0048 で新設) に push する。0048 で `Connection` 構造体に未送信キューが追加された前提で実装する。

### kTLS 有効化完了後の遷移 (`handle_setsockopt_complete`)

`submit_enable_ktls` は SetSockOpt × 3 の CQE で完了する非同期処理。完了時点で `state` を以下のように分岐させる:

- `conn.pending_writes.is_empty()` → 従来通り `submit_read` を発行 (`state = Reading`)
- 非空 → 先頭エントリを `current_write` 系フィールドに移して `submit_write` を発行 (`state = Writing` または `Closing`)

これにより leftover に複数 Request が含まれていても 0048 の対応で順次送信できる。

### L713-746 経路の同型修正

書き込み経路の `is_handshaking() == false` 判定後にも同じ drain + feed 処理を入れる。発火頻度は低いが防御的措置として対称化する。

### エラーハンドリング方針

- `read_to_end` の `WouldBlock` は正常 (drain 完了とみなす)
- それ以外の `io::Error` は当該接続のみ `close_connection` で閉じ、サーバプロセスは継続させる (`?` で main まで伝播させない)
- `conn.decoder.feed(&leftover)` のエラーも同様に当該接続のみクローズ

### テスト

io_uring + kTLS + Linux 環境必須のため CI 自動化は困難。手動再現手順を `examples/http11_server_io_uring/README.md` に追記する:

```sh
# TLS 1.3 で curl が Finished + Application Data を同一 flight で送るシナリオ
curl -v --http1.1 --tlsv1.3 https://localhost:8443/info
```

期待: 修正前は parse error / hang / 接続切断、修正後は正常レスポンスが返る。

さらに `openssl s_client` で TLS 1.3 ハンドシェイク後に手動で HTTP リクエストを送るシナリオも検証する。

### CHANGES.md

サンプルのデータ破損バグ修正は機能に直接影響するため、`### misc` ではなく本体 `[FIX]` 配下に配置する (0048 と方針統一):

```
- [FIX] `examples/http11_server_io_uring` で kTLS 移行時に rustls の復号済み平文を取りこぼし HTTP リクエストの先頭バイトが消失する問題を修正する
  - 旧実装は `tls_conn.is_handshaking() == false` 確認直後に `tls_conn.reader().read(...)` を呼ばずに `dangerous_extract_secrets()` を呼んで `tls_conn` を drop していた
  - TLS 1.3 で Client Finished と Application Data が同一 TCP read で来た場合 (curl / openssl s_client の典型的挙動)、`received_plaintext` に保持された HTTP リクエスト先頭バイトが復元不能で消失していた (TCP 再送経路もないため決定的に発生)
  - `tls_conn.take()` 直後 / `dangerous_extract_secrets()` 直前に `tls_conn.reader().read_to_end(&mut leftover)` で平文を吸い出し、`conn.decoder.feed(&leftover)` で取り込んだ上で 0048 で新設する `pending_writes` に Response を積む
  - L603-636 (Read 完了経路) と L713-746 (Write 完了経路) の両方に同型修正を入れる
  - エラーは当該接続のクローズで局所化し、サーバプロセスは継続させる
  - @voluntas
```

### ブランチ

`feature/fix-io-uring-ktls-plaintext-leftover-drop` (`feature/fix-` prefix、サンプル内の修正のみで本体 API には影響なし、issue 番号を含まない)。

## 受け入れ基準

- L603-636 経路に `tls_conn.reader().read_to_end(...)` での leftover drain が追加されている
- L713-746 経路にも同型の drain が追加されている
- leftover が非空のとき `conn.decoder.feed(&leftover)` で取り込み、`pending_writes` (0048) に Response が積まれている
- `read_to_end` の `WouldBlock` 以外のエラー、`decoder.feed` のエラーは当該接続クローズで局所化され、サーバプロセスは停止しない
- `handle_setsockopt_complete` 完了時に `pending_writes` の状態で `submit_write` か `submit_read` を分岐させている
- `examples/http11_server_io_uring/README.md` に手動テスト手順 (`curl --tlsv1.3` ベース) が追記されている
- CHANGES.md `## develop` に `[FIX]` エントリが追加されている (本体配下、`### misc` ではない)

## RFC / 仕様参照

- RFC 8446 §4.4.4 (TLS 1.3 Finished + Application Data の同一 flight 送信は通常運用)
- rustls API doc (`Reader::read_to_end` の `WouldBlock` 動作、`dangerous_extract_secrets` の前提)
- Linux kTLS doc (kTLS 有効化後はカーネル経由でしか復号できないこと)

## 補足

- TCP レベルでは Client Finished + Application Data の TCP セグメントは ACK 済みのため、クライアントは再送しない。アプリ層で `received_plaintext` を排出しない限り該当バイトは復元不能
