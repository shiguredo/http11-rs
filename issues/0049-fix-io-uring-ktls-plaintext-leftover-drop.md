# 0049: io_uring サンプルで kTLS 移行時に rustls の復号済み平文を取りこぼす問題を修正する

Created: 2026-05-12
Model: Opus 4.7

## 概要

`examples/http11_server_io_uring/src/main.rs` の TLS ハンドシェイク完了検出経路 (L603-636 と L713-746 の 2 箇所) で、`tls_conn.is_handshaking() == false` を確認した直後に `tls_conn.reader().read(...)` を呼ばずに `dangerous_extract_secrets()` → kTLS 有効化 → `tls_conn.take()` で drop している。

```rust
// examples/http11_server_io_uring/src/main.rs:603-630
if !tls_conn.is_handshaking() {
    info!(peer_addr = %peer_addr, "TLS handshake completed");

    let cipher_suite = tls_conn.negotiated_cipher_suite().unwrap();

    let conn = &mut connections[conn_id];
    let tls_conn = conn.tls_conn.take().unwrap();        // L612: take して shadow
    let secrets = tls_conn
        .dangerous_extract_secrets()                      // L613-615: secrets 抽出
        .map_err(|e| format!("failed to extract TLS secrets: {:?}", e))?;
    // ... 以降 ktls_tx / ktls_rx の生成 ...

    conn.state = ConnectionState::EnablingKtls;
    submit_enable_ktls(ring, conn_id, connections)?;
}
// shadow された tls_conn (ServerConnection) はここでスコープを抜けて drop
// rustls 内部に残っていた復号済み plaintext は失われる
```

TLS 1.3 ではハンドシェイク完了時に rustls が **既に Application Data を復号して内部バッファに保持している** 場合がある。Client Finished と HTTP リクエストの平文が同一 TCP read で来ると、`process_new_packets` で復号された平文が `received_plaintext` に格納される。本実装は `tls_conn.reader()` で平文を排出することなく `take()` で drop しているため、その平文は失われる。kTLS は今後到着する新規 TCP データのみ復号するため、消えたバイト列は復元不能。

`tls_conn.reader().read(...)` の呼び出しは grep 0 件で、handshake 完了後の leftover 吸い出し経路が完全に欠落している。

## 根拠

### 発火条件 (決定的、確率的ではない)

1. クライアント: ClientHello → 受信 (ServerHello/Certificate/CertVerify/Finished) → 送信 (ClientFinished + 1-RTT で HTTP GET)
2. サーバ受信: 1 回の `read` で ClientFinished + Application Data を読む (curl / openssl s_client / 多くの HTTP/1.1 クライアントが普通に行う動作)
3. `tls_conn.read_tls` で投入 → `process_new_packets` で復号 → Application Data が rustls 内部の `received_plaintext` に格納
4. `tls_conn.is_handshaking() == false` (Finished も処理済み)
5. 本実装: `tls_conn.take().unwrap()` → ローカル `tls_conn` に move → `dangerous_extract_secrets()` を呼ぶ → ローカル `tls_conn` がスコープ終了で drop → **rustls 内部の HTTP リクエスト先頭バイトが消失**
6. `submit_enable_ktls` で kTLS 有効化 → カーネルが TLS 層を担当
7. クライアントが既に送信した N バイトは二度と来ない → タイムアウト / Parse Error / 接続切断

### AGENTS.md との衝突

- 「サンプルは **お手本** なので性能と堅牢性を両立させること」
- TLS 1.3 + kTLS という難度の高い機能を謳う以上、leftover の取り扱いは必須の設計責務

### workspace exclude による隠蔽

- `Cargo.toml:20` で `exclude = ["examples/http11_server_io_uring", ...]`
- `.github/workflows/ci.yml` で `cargo check` / `clippy` / `test` の対象外
- 致命的バグが CI で検出されない構造的問題

## 影響範囲

- TLS 1.3 + kTLS で HTTP リクエストの先頭バイトが決定的に消失
- 短い HTTP リクエスト (HEAD / GET / 小さな POST) ではリクエスト全体が消える可能性
- お手本サンプルとしての信頼性が崩壊

## 対応方針

### `tls_conn.take()` 直前に平文 drain を追加

```rust
let conn = &mut connections[conn_id];
let mut tls_conn = conn.tls_conn.take().unwrap();
let mut leftover = Vec::new();
tls_conn.reader().read_to_end(&mut leftover).ok();  // EWOULDBLOCK 等は無視
if !leftover.is_empty() {
    conn.decoder.feed(&leftover)?;
}
let secrets = tls_conn.dangerous_extract_secrets()?;
// ... 以降 既存処理 ...
```

### `submit_enable_ktls` 完了後の合流

- leftover が非空のときは `submit_read` ではなく `handle_read` 経路にそのまま合流させ、追加の TCP 受信なしで既存平文を処理する

### L713-746 の同型修正

- 書き込み経路でも同じパターンで leftover を drain する

### workspace への復帰検討

- `Cargo.toml:20` の `exclude` から `examples/http11_server_io_uring` を外し、Linux 限定の cfg を付与
- CI で Linux 専用 job を追加して `cargo check` / `cargo clippy` を最低限通す

### テスト

- io_uring + kTLS 環境必須なので CI 自動化は困難だが、curl で TLS 1.3 セッションを張って HEAD/GET のリクエスト本文末尾が壊れないことを確認するシェルテストを追加する

### CHANGES.md

`## develop` の `### misc` に `[FIX]` として追加する。

### 関連 issue

- 0048 (io_uring パイプライン 2 件目以降ドロップ) と同じファイルの問題群
