# io_uring サーバーが kTLS 移行時に暗号化 leftover を取りこぼす

- Priority: High
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-io-uring-ktls-encrypted-leftover
- Polished: {YYYY-MM-DD}

## 目的

`examples/http11_server_io_uring` で TLS ハンドシェイク完了時に `read_tls` が消費しきれなかった暗号化バイトがローカル変数とともに drop され、kTLS 移行後に復号できなくなる問題を修正する。issue 0049 で平文 leftover は対策済みだが、暗号化レコードのカーソル残量は未対応である。

## 優先度根拠

High とする。TLS 1.3 では Client Finished と Application Data が同一 TCP セグメントで到着する可能性がある。暗号化 leftover を取りこぼすと、HTTP リクエストの先頭バイトが決定的に消失し、接続を再確立する以外に回復できない。特に curl / openssl s_client の典型挙動で発生しうる。

## 現状

`examples/http11_server_io_uring/src/main.rs:638-735` の `handle_read` HandshakeReading ブランチでは以下の処理を行っている。

```rust
let data = conn.read_buf[..bytes_read].to_vec();
...
let mut rd = std::io::Cursor::new(&data);
match tls_conn.read_tls(&mut rd) {
    Ok(_) => {}
    Err(e) => { ... }
}
```

`data` はローカル変数であり、`read_tls` 後に `rd.position()` と `data.len()` を比較していない。同一 TCP read 内で Client Finished の直後に Application Data レコードが来た場合、`read_tls` は Application Data も部分的に消費するが、末尾に不完全なレコードが残る。その残りが `data` 内に残ったまま関数を抜けて drop される。ソケットは既にその分読み進めているため、kTLS 移行後に再取得できない。

`handle_write` HandshakeWriting ブランチ（`src/main.rs:829-884`）でも同様に、ハンドシェイク書き込み完了を契機に kTLS を有効化するが、`read_buf` 内の未処理暗号化データの存在を確認・継続処理していない。

## 設計方針

1. `read_tls` 後に `Cursor::position()` と `data.len()` を比較し、未消費の暗号化バイトが残っていれば追加の `read_tls` / `process_new_packets` を繰り返す。
2. `process_new_packets` により平文化されたデータは issue 0049 と同様に `drain_and_feed_leftover` でデコーダーに取り込む。
3. HandshakeWriting 完了経路でも、暗号化 leftover が残っていないことを確認してから kTLS 移行する。
4. 暗号化 leftover の検出と処理を両経路で共通化するヘルパー関数を検討する。

## 完了条件

- TLS ハンドシェイク完了時に、`read_tls` が消費しきれなかった暗号化バイトが drop されず、復号処理に引き継がれること。
- Client Finished と Application Data が同一 TCP セグメントで到着しても、HTTP リクエストの先頭バイトが消失しないこと。
- 既存の issue 0049 の平文 leftover 対策と両立すること。
- 該当ケースを検証するテストまたは fuzz target が追加されること（可能であれば）。

## 解決方法

- `src/main.rs` の `handle_read` / `handle_write` の TLS ハンドシェイク完了処理で、以下を実装する:
  - `let consumed = rd.position() as usize;`
  - `consumed < data.len()` の場合、`data[consumed..]` を再度 `Cursor` で包み `read_tls` / `process_new_packets` を繰り返す。
  - ループ内で `process_new_packets` の結果、平文が生成されれば `drain_and_feed_leftover` でデコーダーに feed する。
  - 完全に消費するか、不完全なレコードのみが残る場合は次の read イベントまで待つ（ただし既読分はローカルに保持し続ける必要がある）。
- 暗号化 leftover の管理に `conn` 構造体に一時バッファを追加するか、あるいは `read_buf` を消費済み位置で管理する方式を検討する。
- 単体テストが難しい場合は、少なくとも該当経路を検出する fuzz target を追加する。
