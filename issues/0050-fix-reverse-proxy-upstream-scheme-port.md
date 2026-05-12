# 0050: reverse_proxy サンプルで upstream URL の scheme と port を尊重する

Created: 2026-05-12
Model: Opus 4.7

## 概要

`examples/http11_reverse_proxy/src/main.rs` の `parse_upstream_url` (L731-752) は host のみ返し、scheme と port を完全に捨てている。さらに `TcpStream::connect((host, 443))` (L149) で **常に 443 へ TLS 接続** している。

```rust
// examples/http11_reverse_proxy/src/main.rs:731-752
fn parse_upstream_url(url: &str) -> Result<String, Box<dyn std::error::Error>> {
    let url_str = if let Some(rest) = url.strip_prefix("https://") {
        rest
    } else if let Some(rest) = url.strip_prefix("http://") {
        rest
    } else {
        url
    };

    let host = url_str
        .split('/').next().ok_or("Invalid URL: no host")?
        .split('?').next().ok_or("Invalid URL: no host")?
        .split(':').next().ok_or("Invalid URL: no host")?;    // ← port を完全に捨てる

    Ok(host.to_string())   // ← 戻り値は String 1 つだけ、scheme / port を返さない
}
```

```rust
// examples/http11_reverse_proxy/src/main.rs:148-150
let server_name = ServerName::try_from(host.to_string())?;
let tcp_stream = TcpStream::connect((host, 443)).await?;
let tls_stream = tls_connector.connect(server_name, tcp_stream).await?;
```

`--upstream http://internal:8080/api` のような実用的な指定は **完全に機能しない** (host だけが抽出され、port=443 で接続、TLS ハンドシェイク強行)。help text は `Upstream URL (default: https://example.com)` と書いているが、URL の体裁を受け付けない。

## 根拠

### 動作確認

- `--upstream http://internal:8080/api`:
  - `parse_upstream_url` → `"internal"`
  - `TcpStream::connect(("internal", 443))` → 443 で TCP 接続試行
  - `tls_connector.connect(...)` → TLS ハンドシェイク試行
  - 実際の upstream が 8080 で plaintext を提供している場合は全 シナリオ で動作不能
- `--upstream https://example.com:8443`:
  - 同様に port 8443 が捨てられ、443 へ接続される

### AGENTS.md との衝突

- 「サンプルは **お手本** なので性能と堅牢性を両立させること」
- 利用者が CLI 引数を信用できない時点でお手本の体をなさない

## 影響範囲

- `--upstream` オプションの全シナリオで機能不全
- HTTPS upstream で port が 443 以外の場合に接続不能
- HTTP (plaintext) upstream は常に接続不能
- reverse proxy として実用にならない

## 対応方針

### `parse_upstream_url` の戻り値拡張

- `(scheme: Scheme, host: String, port: u16)` を返すよう構造化する
- `Scheme` は `enum { Http, Https }` 等で表現
- port のデフォルトは scheme から決定 (`http` = 80, `https` = 443)、URL に明示的な `:port` があればそれを使う

### `create_connection` / `stream_upstream_response_pooled`

- `scheme` で plaintext (TCP のみ) と TLS (TCP + tokio_rustls) を切り替える
- `TcpStream::connect((host, port))` で動的な port を使う
- 接続プールキーも `(scheme, host, port)` 化する

### 関連メンテナンス

- `--upstream http://...` を許可するか `--upstream https://...` 専用に絞るかの方針決定
- help text を実態に合わせて更新する

### テスト

- `examples/http11_reverse_proxy` の integration test (現状不在) で、`http://` upstream と `https://` upstream の両方で end-to-end が動くことを確認する

### CHANGES.md

`## develop` の `### misc` に `[FIX]` として追加する。

### 関連 issue

- 0051 (reverse_proxy CONNECT 未対応) と並んで reverse_proxy の機能不全 3 連
