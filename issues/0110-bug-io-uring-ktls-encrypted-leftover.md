# io_uring サーバーが kTLS 移行時に暗号化 leftover を取りこぼす

- Priority: High
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-io-uring-ktls-encrypted-leftover
- Polished: 2026-06-13

## 目的

`examples/http11_server_io_uring` で TLS ハンドシェイク完了時に `read_tls` が消費しきれなかった暗号化バイトを kTLS 移行後も復号処理に引き継ぐ。issue 0049 で平文 leftover は対策済みだが、暗号化レコードのカーソル残量は未対応である。

## 優先度根拠

High とする。TLS 1.3 では Client Finished と Application Data が同一 TCP セグメントで到着する可能性がある。暗号化 leftover を取りこぼすと、HTTP リクエストの先頭バイトが決定的に消失し、接続を再確立する以外に回復できない。

## 現状

`handle_read` の `ConnectionState::HandshakeReading` ブランチ（`examples/http11_server_io_uring/src/main.rs` L639 付近）では以下の処理を行っている。

```rust
let data = conn.read_buf[..bytes_read].to_vec();
let tls_conn = conn.tls_conn.as_mut().unwrap();
let mut rd = std::io::Cursor::new(&data);
match tls_conn.read_tls(&mut rd) {
    Ok(_) => {}
    Err(e) => { ... }
}
match tls_conn.process_new_packets() { ... }
```

`read_tls` は `data` 内のバイトを rustls の内部 `deframer_buffer` に可能な限りコピーする。`Cursor` 上では通常 `data.len()` まで進み、未消費バイトは `data` 内に残らない。しかし、内部 `deframer_buffer` に不完全な TLS レコードが残った状態で `tls_conn` が drop されると、その不完全レコードが破棄される。

TLS 1.3 で Client Finished と Application Data が同一 TCP セグメントに届いた場合、Application Data レコードが TCP セグメント末尾で分割されていれば、`read_tls` はその断片を内部バッファにコピーするが、`process_new_packets` は不完全なため復号できない。ハンドシェイク完了直後に `dangerous_extract_secrets()` を呼んで `tls_conn` を drop すると、この不完全 Application Data レコードが失われ、kTLS 移行後に復号不能になる。TCP レベルでは ACK 済みのため再送もない。

## 設計方針

1. `Connection` 構造体に暗号化 leftover を保持するフィールド `encrypted_leftover: Vec<u8>` を追加する。ハンドシェイク完了・kTLS 移行までの間に、不完全な TLS レコードの断片をここに蓄積する。

2. TLS レコード層の境界を自前で解析し、完全なレコードのみを `read_tls` に渡す。TLS レコードヘッダーは 5 バイト（content_type: 1、version: 2、length: 2）であり、`length` フィールドからペイロード長を取得して完全レコードサイズ `5 + length` を計算する。連結バッファ `data` 内で完全レコードサイズに満たない末尾部分は `encrypted_leftover` に保持する。

3. `handle_read` の `HandshakeReading` ブランチを以下のように改修する。
   - `let mut data = std::mem::take(&mut conn.encrypted_leftover);`
   - `data.extend_from_slice(&conn.read_buf[..bytes_read]);`
   - `data` 内の完全な TLS レコードを `read_tls` に順次投入する。不完全な末尾部分は `conn.encrypted_leftover` に戻す。
   - 各完全レコード投入後に `process_new_packets` を呼び、生成された平文があれば `drain_and_feed_leftover` でデコーダーに feed する。
   - `wants_write()` チェックは各レコード投入・処理後に行い、true なら `submit_write` を発行して return する。次の `handle_read` / `handle_write` 発火時に `encrypted_leftover` と新規 `read_buf` を連結して処理を継続する。

4. `handle_read` / `handle_write` の両方で、ハンドシェイク完了後の kTLS 移行直前（`dangerous_extract_secrets()` 呼び出し前）に `encrypted_leftover.is_empty()` を確認する。空でない場合は kTLS 移行を遅らせ、`conn.state = ConnectionState::HandshakeReading` に戻して `submit_read` を発行し、次の read で不完全レコードの後続バイトを受信してから再試行する。

## 完了条件

- `Connection` 構造体に `encrypted_leftover: Vec<u8>` フィールドを追加し、初期値を空にすること。
- TLS ハンドシェイク完了時に、不完全な TLS レコードの断片が `tls_conn` drop によって失われず、次の read イベントで `read_tls` に再投入されること。
- `handle_read` / `handle_write` の両方で、kTLS 移行直前（`dangerous_extract_secrets()` 呼び出し前）に `encrypted_leftover.is_empty()` を確認し、空でない場合は kTLS 移行を遅らせて追加 read を待つこと。
- kTLS 移行後（または直前の最終確認時）に `encrypted_leftover` をクリアし、以降の HTTP リクエスト処理に影響を与えないこと。
- 既存の issue 0049 の平文 leftover 対策（`drain_and_feed_leftover`）と両立し、同一 read 内で生成された平文も確実にデコーダーに feed すること。
- `examples/http11_server_io_uring/README.md` に手動再現手順を追記すること。例:
  - `READ_BUF_SIZE` を一時的に小さく（例: 128 バイト）して、Client Finished + Application Data が複数回の `io_uring::Read` に分割されるように強制する。
  - あるいは `openssl s_client -connect localhost:8443 -tls1_3 -quiet` で接続後、TLS ハンドシェイク完了直後に大きな HTTP リクエストを送信し、Application Data レコードが TCP セグメント境界で分割されるようにする。
  - 期待: 修正前は parse error / hang / 接続切断、修正後は正常レスポンスが返る
- `CHANGES.md` の `## develop` セクションに `[FIX]` エントリを追加すること（サンプルのデータ破損バグ修正なので本体 `[FIX]` 配下、`### misc` ではない）。例:
  - [FIX] `examples/http11_server_io_uring` で kTLS 移行時に不完全な TLS レコードの断片を取りこぼし HTTP リクエストの先頭バイトが消失する問題を修正する
    - 旧実装は `read_tls` で内部 deframer buffer にコピーした不完全レコードを、`dangerous_extract_secrets()` 呼び出し後の `tls_conn` drop で破棄していた
    - TLS 1.3 で Client Finished と Application Data が同一 TCP read で来た場合、Application Data レコードが TCP セグメント末尾で分割されていると、その断片が復元不能で消失していた (TCP 再送経路もないため決定的に発生)
    - `Connection` に `encrypted_leftover: Vec<u8>` を追加し、完全な TLS レコードのみを `read_tls` に投入して不完全レコードの断片を外部に保持し、次の read で再投入する
    - kTLS 移行直前に `encrypted_leftover.is_empty()` を確認し、空でない場合は追加 read を待って移行を遅らせる
    - issue 0049 の `drain_and_feed_leftover` による平文 leftover 対策と両立させる
    - @voluntas

## 解決方法

- `Connection` 構造体（L122 付近）に `encrypted_leftover: Vec<u8>` フィールドを追加する。初期値は空。kTLS 移行直前に `.clear()` または `std::mem::take` して状態をリセットする。
- `handle_read` の `HandshakeReading` ブランチ（L639 付近）を設計方針に従って改修する。
  - `let mut data = std::mem::take(&mut conn.encrypted_leftover);`
  - `data.extend_from_slice(&conn.read_buf[..bytes_read]);`
  - 連結バッファから完全な TLS レコードを順次 `read_tls` に投入する。`Cursor` を使って 1 レコードずつ切り出し、不完全レコードが来た時点で停止する。
  - 各レコード投入後に `process_new_packets` を呼び、生成された平文があれば issue 0049 の `drain_and_feed_leftover`（L575 付近）でデコーダーに feed する。
  - `wants_write()` が true なら `submit_write` を発行して return する。
  - 不完全レコードの断片は `conn.encrypted_leftover` に戻す。
- `handle_write` の `HandshakeWriting` ブランチ（L829 付近）でも、ハンドシェイク完了時に `encrypted_leftover.is_empty()` を確認する。空でない場合は `conn.state = ConnectionState::HandshakeReading` に戻して `submit_read` を発行し return する。空なら issue 0049 と同様に `drain_and_feed_leftover` を呼んでから kTLS 移行する。
- `examples/http11_server_io_uring/README.md` に手動再現手順を追記する。
- `CHANGES.md` に `[FIX]` エントリを追加する。

## RFC / 仕様参照

- RFC 8446 §4.4.4 (TLS 1.3 で Client Finished と Application Data が同一 flight で送信される typical な運用)
- rustls API doc (`read_tls` は内部 deframer buffer へコピーし、`process_new_packets` で完全レコードを取り出す; `dangerous_extract_secrets` 呼び出し後は `tls_conn` drop で内部バッファが失われる)
- Linux kTLS doc (kTLS 有効化後はカーネル経由でしか復号できないこと)
