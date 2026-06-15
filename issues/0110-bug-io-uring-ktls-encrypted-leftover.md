# io_uring サーバーが kTLS 移行時に暗号化 leftover を取りこぼす

- Priority: High
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-io-uring-ktls-encrypted-leftover
- Polished: 2026-06-16

## 目的

`examples/http11_server_io_uring` で TLS ハンドシェイク完了時に、rustls の内部 deframer buffer に残された **復号未完の不完全 TLS レコード断片** が `tls_conn` drop によって失われる問題を修正する。closed/0049 で対応した平文 leftover (rustls の `received_plaintext`) と本 issue で対応する暗号化 leftover (`deframer_buffer`) は **互いに独立** で、closed/0049 では対象外だった層。本 issue 完了でハンドシェイク直後の暗号化バイトすべてが kTLS 移行後の復号処理に引き継がれるようになる。

なお `examples/http11_server_io_uring` は workspace から `exclude` されており、`cargo check --workspace` / `cargo test --workspace` の自動検査対象外である (`Cargo.toml:20`)。CI のスモークから外れているため手動検証が前提となる点を README の手動再現手順で補完する。

## 優先度根拠

High とする。TLS 1.3 では Client → Server 方向で Client Finished と Application Data が同一 TCP セグメントで到着する典型ケース (RFC 8446 §4.4.4 で client Authentication Finished の直後に application data を送れる) がある。暗号化 leftover を取りこぼすと、HTTP リクエストの先頭バイトが決定的に消失し、接続を再確立する以外に回復できない。TCP レベルでは ACK 済みのため再送経路もない。

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

### 代替案検討

本 issue では「TLS レコード層境界を自前パースして完全レコードのみ `read_tls` に渡し、断片を `Connection::encrypted_leftover` に外出しする」方針を採る。検討した代替案と却下理由:

- **rustls `complete_io`**: TCP ソケット (`Read + Write`) を直接受け取って handshake を完了させる API。`io_uring` の completion-based イベントモデルでは `complete_io` を呼ぶ間ソケットをブロックすることになり、本サンプル (multi-connection accept loop) と相性が悪い
- **kTLS 移行を遅らせて `tls_conn` の内部 deframer buffer に追加 read を流し込む**: kTLS 移行を遅延させても `read_tls` のソース (`Cursor<&[u8]>`) は呼び出しごとに新しい一過性データであり、追加 read を rustls 内部 deframer に渡し続ける限り leftover は溜まり続けるだけで本質的解決にならない。さらに `tls_conn` 保持中は kTLS 経路に切り替えできないため、HTTP ハンドラ起動も遅延する

上記の通り、`io_uring` の completion モデルと kTLS 性能要件を両立させるには「自前レコード境界パース + 断片の外出し保持」が現実的解。

### 具体方針

1. `Connection` 構造体 (`L122`) に暗号化 leftover を保持するフィールド `encrypted_leftover: Vec<u8>` を追加する。初期値は空。`Connection` には専用 `new` 関数が無いためリテラル初期化箇所 (`L157` 周辺、`Connection { ..., state: ConnectionState::HandshakeReading, ... }`) に `encrypted_leftover: Vec::new()` を追加する。

2. TLS レコード層の境界を自前で解析し、完全なレコードのみを `read_tls` に渡す。専用 helper を追加する。

   ```rust
   /// 連結バッファの先頭から完全な TLS レコード境界 (= 末尾までのバイト数) を返す。
   /// 不完全な場合は None。RFC 8446 Section 5.1 / 5.2 の制約に従う。
   fn complete_record_end(buf: &[u8]) -> Result<Option<usize>, RecordError> {
       if buf.len() < 5 { return Ok(None); }  // header 未満は不完全
       // TLS 1.2/1.3 共通: content_type (1) + version (2) + length (2 big-endian)
       let length = u16::from_be_bytes([buf[3], buf[4]]) as usize;
       // 上限 2^14 + 256 は RFC 8446 §5.2 TLSCiphertext の値。TLSPlaintext (§5.1 上限 2^14) も
       // この上位互換となるため、レコード種別を判別せず単一上限で扱う (本サンプルが
       // 受け取るのはハンドシェイク後の Application Data が主だが、冒頭の平文 ClientHello も
       // この境界内に収まることが保証される)。
       if length > (1 << 14) + 256 { return Err(RecordError::RecordTooLarge { length }); }
       let total = 5 + length;
       if buf.len() < total { Ok(None) } else { Ok(Some(total)) }
   }
   ```

3. `handle_read` の `HandshakeReading` ブランチ (`L639`) を以下の擬似コードに従い改修する。

   ```text
   let mut data = std::mem::take(&mut conn.encrypted_leftover);
   data.extend_from_slice(&conn.read_buf[..bytes_read]);

   let mut cursor = 0usize;
   loop {
       match complete_record_end(&data[cursor..]) {
           Ok(None) => break,                              // 不完全レコードに到達: ループ終了
           Err(RecordError::RecordTooLarge { length }) => { // 上限超過は protocol error
               log::error!("TLS record length exceeds 2^14+256 limit: {length}");
               // 既存の close_connection 経路に乗せる (Closing 遷移)
               conn.state = ConnectionState::Closing;
               submit_close(...);
               return;
           }
           Ok(Some(total)) => {
               let record = &data[cursor..cursor + total];
               let mut rd = std::io::Cursor::new(record);
               tls_conn.read_tls(&mut rd)?;
               cursor += total;

               match tls_conn.process_new_packets() {
                   Ok(io_state) if io_state.peer_has_closed() => {
                       conn.state = ConnectionState::Closing;
                       submit_close(...);
                       return;
                   }
                   Ok(_) => { /* 続行 */ }
                   Err(e) => { /* 既存のエラー処理経路に乗せる */ }
               }

               drain_and_feed_leftover(&mut tls_conn, conn, peer_addr)?;

               if tls_conn.wants_write() {
                   conn.encrypted_leftover = data[cursor..].to_vec(); // 続きを保持して return
                   submit_write(...);
                   return;
               }
           }
       }
   }
   // ループ脱出後、未消費の不完全レコード断片を保持
   conn.encrypted_leftover = data[cursor..].to_vec();
   ```

   要点:
   - `Ok(None)` で break、`Err(RecordTooLarge)` で `Closing` 遷移、`Ok(Some(total))` で 1 レコード処理して `cursor` を進める
   - 各レコード処理後の `wants_write()` チェックで途中保存する場合は、未消費部分を必ず `encrypted_leftover` に書き戻してから return する
   - ループ脱出後 (全完全レコード処理済み) も末尾の不完全断片を `encrypted_leftover` に保持する

4. `handle_read` / `handle_write` の両方で、ハンドシェイク完了後の kTLS 移行直前 (`dangerous_extract_secrets()` 呼び出し前、`L712` / `L862`) に `encrypted_leftover.is_empty()` を確認する。空でない場合は kTLS 移行を遅らせる。その際、TLS の未送信出力 (`tls_conn.wants_write()`) の取り扱いを以下の順序で処理する。
   - まず `tls_conn.wants_write()` を確認し、true なら先に `submit_write` を発行して return する (次の `handle_write` で再評価される)
   - `wants_write()` が false なら `conn.state = ConnectionState::HandshakeReading` に戻して `submit_read` を発行し、次の read で不完全レコードの後続バイトを受信してから再試行する
   - これにより Server Finished 等の TLS 出力を取り残したまま `HandshakeReading` 復帰してデッドロックする経路を防ぐ

5. kTLS 有効化 (`ConnectionState::EnablingKtls` 遷移時) で `encrypted_leftover` を `Vec::new()` にクリアして状態をリセットする。クリアタイミングは `dangerous_extract_secrets()` の **直前** (kTLS 移行を確定する直前) に `debug_assert!(conn.encrypted_leftover.is_empty())` で不変条件を検証してから、`std::mem::take(&mut conn.encrypted_leftover)` で安全に空に落とす。

## 完了条件

- `Connection` 構造体に `encrypted_leftover: Vec<u8>` フィールドが追加され、リテラル初期化箇所 (現状 `L157` 周辺) で空 `Vec::new()` で初期化されること。
- `complete_record_end` helper が `src/main.rs` 内に追加され、RFC 8446 §5.2 の `TLSCiphertext.length <= 2^14 + 256` 上限を超える length を `RecordError::RecordTooLarge` として早期 reject すること。
- TLS ハンドシェイク完了時に、不完全な TLS レコードの断片が `tls_conn` drop によって失われず、次の read イベントで `read_tls` に再投入されること。
- `process_new_packets` の `IoState` で `peer_has_closed()` を観測した場合に `ConnectionState::Closing` 遷移すること。
- `handle_read` / `handle_write` の両方で、kTLS 移行直前 (`dangerous_extract_secrets()` 呼び出し前) に `encrypted_leftover.is_empty()` を確認し、空でない場合は kTLS 移行を遅らせて追加 read を待つこと。
- kTLS 有効化 (`ConnectionState::EnablingKtls` 遷移) 直前で `debug_assert!(conn.encrypted_leftover.is_empty())` を通過し、`std::mem::take` でクリアすること。以降の HTTP リクエスト処理に影響を与えないこと。
- closed/0049 の平文 leftover 対策 (`drain_and_feed_leftover`) と両立し、同一 read 内で生成された平文も確実にデコーダーに feed すること。
- `refs/rfc8446.txt` (TLS 1.3) を本リポジトリに追加すること。AGENTS.md「RFC を確認する際は refs/ 以下を利用すること」を満たすため、本 issue で必須参照になる §4.4.4 / §5.1 / §5.2 を原典で参照可能にする。
- `examples/http11_server_io_uring/README.md` に手動再現手順を追記すること。具体的には以下の手順で再現する。
  1. `examples/http11_server_io_uring/src/main.rs` の `READ_BUF_SIZE` (現状の定数値を README 内に併記) を一時的に小さい値 (例: 128 バイト) にして、Client Finished + Application Data が複数回の `io_uring::Read` に分割されるよう強制する
  2. `cargo run --release --manifest-path examples/http11_server_io_uring/Cargo.toml` でサーバーを起動する
  3. `openssl s_client -connect localhost:8443 -tls1_3 -quiet` で接続後、TLS ハンドシェイク完了直後に `GET / HTTP/1.1\r\nHost: localhost:8443\r\n\r\n` (改行込み 約 40 バイトの長め HTTP リクエスト) を送信し、Application Data レコードが TCP セグメント境界で分割されるようにする
  - 期待: 修正前は parse error / 接続切断、修正後は正常レスポンスが返る
- `CHANGES.md` の `## develop` セクションに `[FIX]` エントリを `[ADD]` の下、`### misc` の上に追加すること (サンプルのデータ破損バグ修正なので本体 `[FIX]` 配下、`### misc` ではない)。例:
  - [FIX] `examples/http11_server_io_uring` で kTLS 移行時に不完全な TLS レコードの断片を取りこぼし HTTP リクエストの先頭バイトが消失する問題を修正する
    - 旧実装は `read_tls` で内部 deframer buffer にコピーした不完全レコードを、`dangerous_extract_secrets()` 呼び出し後の `tls_conn` drop で破棄していた
    - TLS 1.3 で Client Finished と Application Data が同一 TCP read で来た場合、Application Data レコードが TCP セグメント末尾で分割されていると、その断片が復元不能で消失していた (TCP 再送経路もないため決定的に発生)
    - `Connection` に `encrypted_leftover: Vec<u8>` を追加し、完全な TLS レコードのみを `read_tls` に投入して不完全レコードの断片を外部に保持し、次の read で再投入する
    - kTLS 移行直前に `encrypted_leftover.is_empty()` を確認し、空でない場合は追加 read を待って移行を遅らせる
    - closed/0049 の `drain_and_feed_leftover` による平文 leftover 対策と両立させる
    - @voluntas

## 解決方法

設計方針に沿って次の順序で実装する (設計方針節に詳細あり、ここでは順序のみ示す)。

1. `refs/rfc8446.txt` を本リポジトリに追加する (本 issue が引用する §4.4.4 / §5.1 / §5.2 を原典で参照可能にする)
2. `Connection` 構造体 (`L122`) に `encrypted_leftover: Vec<u8>` フィールドを追加し、リテラル初期化箇所 (`L157`) を更新する
3. `complete_record_end` helper と `RecordError` enum を `src/main.rs` 内に追加する (`RecordError::RecordTooLarge { length: usize }` バリアントを持たせ、`log::error!` で診断を残す)
4. `handle_read` の `HandshakeReading` ブランチ (`L639`) を設計方針 3 に従い改修する
5. `handle_write` の `HandshakeWriting` ブランチ (`L829`) でも `encrypted_leftover.is_empty()` を確認し、空でない場合は `HandshakeReading` 復帰を行う
6. kTLS 移行直前の `debug_assert!(conn.encrypted_leftover.is_empty())` と `std::mem::take` を `L712` / `L862` 周辺に追加する
7. `examples/http11_server_io_uring/README.md` に手動再現手順を追記する
8. `CHANGES.md` に `[FIX]` エントリを追加する

## RFC / 仕様参照

- RFC 8446 §4.4.4 Finished (Client Finished 直後に Application Data を送れる典型運用)
- RFC 8446 §5.1 Record Layer (TLSPlaintext の length 上限 2^14)
- RFC 8446 §5.2 Record Payload Protection (TLSCiphertext の length 上限 2^14 + 256)
- rustls 0.23 API doc <https://docs.rs/rustls/0.23/rustls/struct.ConnectionCommon.html#method.read_tls> および <https://docs.rs/rustls/0.23/rustls/struct.ConnectionCommon.html#method.dangerous_extract_secrets> (`read_tls` は内部 deframer buffer へコピーし、`process_new_packets` で完全レコードを取り出す。`dangerous_extract_secrets` 呼び出し後は `tls_conn` drop で内部バッファが失われる)
- Linux kTLS 仕様 <https://www.kernel.org/doc/html/latest/networking/tls.html> (kTLS 有効化後はカーネル経由でしか復号できないこと)

## 参考: RFC 文面

- RFC 8446 §5.1 TLSPlaintext
  > opaque fragment[TLSPlaintext.length];
  > The length (in bytes) of the following TLSPlaintext.fragment. The length MUST NOT exceed 2^14 bytes.
- RFC 8446 §5.2 TLSCiphertext
  > opaque encrypted_record[TLSCiphertext.length];
  > The length (in bytes) of the following TLSCiphertext.encrypted_record, which is the sum of the lengths of the content and the padding, plus one for the inner content type, plus any expansion added by the AEAD algorithm. The length MUST NOT exceed 2^14 + 256 bytes.
- RFC 8446 §4.4.4 Finished
  > The Finished message is the final message in the Authentication Block. It is essential for providing authentication of the handshake and of the computed keys.
  > Note: The application data sent by the client could be sent in the same flight as the Finished message.
