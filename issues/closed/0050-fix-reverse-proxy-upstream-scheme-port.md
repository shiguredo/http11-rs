# 0050: reverse_proxy サンプルで upstream URL の scheme と port を尊重する

Created: 2026-05-12
Completed: 2026-05-12
Model: Opus 4.7

## 概要

`examples/http11_reverse_proxy/src/main.rs` の `parse_upstream_url` (L731-752) は host のみ返し、scheme と port を完全に捨てている。`create_connection` (L148-150) は `TcpStream::connect((host, 443))` で常に 443 へ TLS 接続するハードコード。`--upstream http://internal:8080/api` のような実用的な指定は完全に機能しない (help は `Upstream URL (default: https://example.com)` と書いているのに URL の体裁を受け付けない)。

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
        .split(':').next().ok_or("Invalid URL: no host")?;

    Ok(host.to_string())
}

// examples/http11_reverse_proxy/src/main.rs:148-150
let server_name = ServerName::try_from(host.to_string())?;
let tcp_stream = TcpStream::connect((host, 443)).await?;
let tls_stream = tls_connector.connect(server_name, tcp_stream).await?;
```

`parse_upstream_url` は example 内部の private 関数で公開 API ではなく、本 issue の戻り値型変更は破壊的変更ではない (`fix-` で扱う)。

## 根拠

### 動作確認結果

- `--upstream http://internal:8080/api`: `parse_upstream_url` が `"internal"` を返し、443 へ TLS 接続試行 → 上流が 8080 plaintext なら接続不能
- `--upstream https://example.com:8443`: port 8443 が捨てられ 443 へ接続される
- `--upstream https://[::1]:8443`: IPv6 リテラルが `split(':')` で破壊される (`host = "["`)

### RFC 根拠

- RFC 3986 §3.2.2 (host): IP-literal (`[...]` ブラケット表記) / IPv4address / reg-name
- RFC 3986 §3.2.3 (port): port は `*DIGIT`、空文字も許容
- RFC 9110 §4.2.1 (http URI): デフォルトポート 80
- RFC 9110 §4.2.2 (https URI): デフォルトポート 443
- RFC 9110 §7.2 (Host ヘッダー): `host:port` の形式、scheme のデフォルトポートでは port 省略が正書法、IPv6 はブラケット必須

### AGENTS.md との衝突

- 「サンプルは **お手本** なので性能と堅牢性を両立させること」
- 利用者が CLI 引数を信用できない時点でお手本の体をなさない

## スコープ

- `parse_upstream_url` を `(Scheme, host: String, port: u16)` 返却に拡張
- `create_connection` を scheme で plaintext / TLS に分岐
- 接続プールのキーを `(Scheme, String, u16)` のタプル化
- `PooledConnection` の `stream` フィールドを plaintext / TLS 両対応にするための型整理
- Host ヘッダー組み立て規則の実装
- IPv6 リテラル `[::1]:8443` のサポート
- 含まない:
  - `--upstream` URL の path / query 部の扱い (本 issue では従来通り無視、クライアント要求の path を upstream に転送)
  - 0051 (CONNECT 未対応) / 0052 (close-delimited drain) — 同じファイル対象だが症状は独立、本 issue を先に着手
  - `examples/http11_client::parse_url` との共通化 (依存追加が必要なため別 issue)

## 対応方針

### Scheme 型と `parse_upstream_url` の戻り値

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scheme {
    Http,
    Https,
}

impl Scheme {
    fn default_port(self) -> u16 {
        match self {
            Scheme::Http => 80,    // RFC 9110 §4.2.1
            Scheme::Https => 443,  // RFC 9110 §4.2.2
        }
    }
}

struct UpstreamUrl {
    scheme: Scheme,
    host: String,  // IPv6 リテラルは brackets を **含まない** 裸の host (例: "::1")
    port: u16,
}

fn parse_upstream_url(url: &str) -> Result<UpstreamUrl, Box<dyn std::error::Error>> {
    // 1. scheme をパース ("http://" or "https://" 必須)
    // 2. authority 部 (host[:port]) を path/query の前で切り出す
    // 3. IPv6 リテラル: `[` で始まる場合は `]` までを host、`]:port` で port を切り出す
    // 4. IPv4 / reg-name: `:` を rfind して port を切り出す (path 区切り `/` より前のみ)
    // 5. port 省略時は scheme.default_port() を使用
    // 6. scheme 不在のエラー、不正な IPv6 リテラルのエラーは英語で返す (AGENTS.md 準拠)
}
```

### `PooledConnection.stream` の型整理

plaintext / TLS 両方を保持するため `enum` で分岐する:

```rust
enum UpstreamStream {
    Plain(BufWriter<TcpStream>),
    Tls(BufWriter<TlsStream<TcpStream>>),
}
```

`stream_response_on_connection` のシグネチャは `&mut UpstreamStream` に変更する。`AsyncRead` / `AsyncWrite` の呼び出し箇所は `match` で分岐する (もしくは内部で `Pin<&mut dyn AsyncRead + AsyncWrite + ...>` に変換する補助関数を用意する)。`Box<dyn>` 化は型情報を失うため採用しない。

### `create_connection` の scheme 分岐

```rust
async fn create_connection(scheme: Scheme, host: &str, port: u16, tls: &Arc<TlsConnector>)
    -> Result<UpstreamStream, Box<dyn std::error::Error>>
{
    let tcp = TcpStream::connect((host, port)).await?;
    match scheme {
        Scheme::Http => Ok(UpstreamStream::Plain(BufWriter::new(tcp))),
        Scheme::Https => {
            let server_name = ServerName::try_from(host.to_string())?;
            let tls_stream = tls.connect(server_name, tcp).await?;
            Ok(UpstreamStream::Tls(BufWriter::new(tls_stream)))
        }
    }
}
```

`ServerName::try_from` は TLS 経路のみで呼ぶ。

### 接続プールキー

```rust
type UpstreamKey = (Scheme, String, u16);

struct ConnectionPool {
    idle_connections: HashMap<UpstreamKey, Vec<PooledConnection>>,
    // ...
}
```

`try_acquire(&mut self, key: &UpstreamKey)` / `release(&mut self, key: UpstreamKey, ...)` のシグネチャ変更が必要。`stats()` の `hosts` フィールド名は `endpoints` に改名する。

### Host ヘッダー組み立て規則

`upstream_request.add_header("Host", ...)` で渡す値は以下のルールで構築:

```rust
fn format_host_header(scheme: Scheme, host: &str, port: u16) -> String {
    let host_part = if host.contains(':') {
        // IPv6 リテラル: brackets を付ける
        format!("[{}]", host)
    } else {
        host.to_string()
    };
    if port == scheme.default_port() {
        host_part
    } else {
        format!("{}:{}", host_part, port)
    }
}
```

- デフォルトポート (`http`=80 / `https`=443) は省略 (RFC 9110 §7.2 正書法)
- IPv6 リテラルは Host ヘッダーでブラケット必須
- `TcpStream::connect((host, port))` には裸の host を渡す (`::1`)

### help text の更新

```
--upstream <URL>  Upstream URL with scheme (http or https) and optional port
                  Default: https://example.com
                  Examples: http://internal:8080, https://[::1]:8443
```

### CHANGES.md

サンプルの機能不全修正は機能に直接影響するため `### misc` ではなく本体 `[FIX]` 配下 (0048 / 0049 と方針統一):

```
- [FIX] `examples/http11_reverse_proxy` で `--upstream` の scheme と port を尊重するよう修正する
  - 旧実装は `parse_upstream_url` が host のみ抽出し、`TcpStream::connect((host, 443))` で常に 443 への TLS 接続にハードコードしていた
  - `Scheme` enum (`Http` / `Https`) と `UpstreamStream` enum (`Plain` / `Tls`) を導入し、`create_connection` を scheme で分岐する
  - 接続プールのキーを `(Scheme, host, port)` のタプルに変更し、`http://` と `https://` の接続が混在しないようにする
  - IPv6 リテラル `[::1]:8443` を正しくパースする
  - Host ヘッダー値はデフォルトポートで省略、IPv6 リテラルはブラケット表記で構築する (RFC 9110 §7.2)
  - @voluntas
```

### ブランチ

`feature/fix-reverse-proxy-upstream-scheme-port` (`feature/fix-` prefix、example 内部の修正のみで本体 API には影響なし、issue 番号を含まない)。

## 受け入れ基準

- `--upstream http://internal:8080/` で plaintext TCP 接続が確立し HTTP リクエストが転送される
- `--upstream https://example.com:8443/` で 8443 へ TLS 接続が確立する
- `--upstream https://example.com/` (port 省略) で 443 へ TLS 接続する
- `--upstream http://example.com/` (port 省略) で 80 へ plaintext 接続する
- `--upstream https://[::1]:8443/` の IPv6 リテラルが正しく解決される
- `--upstream` に scheme が含まれない (`example.com:8080`) 場合は明示的なエラーメッセージで reject される
- 接続プールが `(Scheme, host, port)` ごとに分離されており、`http://a:8080/` と `https://a:443/` のプールエントリが混在しない
- Host ヘッダーがデフォルトポート省略 + IPv6 ブラケット表記で組み立てられている
- `--upstream` の help text が更新されている
- `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` がすべて PASS
- CHANGES.md `## develop` に `[FIX]` エントリが追加されている

## 関連 issue

- 0051 (CONNECT 未対応): 同じファイル対象、本 issue マージ後に rebase
- 0052 (close-delimited drain): 同じファイル対象、本 issue マージ後に rebase

3 issue は同一ファイル (`examples/http11_reverse_proxy/src/main.rs`) の独立した症状で、0050 を先に着手し、続いて 0051 → 0052 の順で進める。

## RFC 参照

- RFC 3986 §3.2.2 / §3.2.3 (host / port、`refs/rfc3986.txt` は本リポジトリ未収載のため別途参照)
- RFC 9110 §4.2.1 / §4.2.2 / §7.2 (http(s) URI、Host ヘッダー、`refs/rfc9110.txt`)

## 解決方法

- `examples/http11_reverse_proxy/src/main.rs` に `Scheme` enum (`Http` / `Https`) と `UpstreamUrl` 構造体 (`scheme` / `host` / `port`) を追加した
- `parse_upstream_url` の戻り値を `Result<UpstreamUrl, _>` に拡張し、scheme / port / IPv6 リテラル (`[host]:port`) / path / query を正しくパースする実装に書き換えた。scheme 不在は明示的にエラー化する
- Host ヘッダー値を組み立てるヘルパー `format_host_header(scheme, host, port)` を追加し、デフォルトポートは省略・IPv6 リテラルはブラケット表記で構築する (RFC 9110 Section 7.2)
- `UpstreamStream` enum (`Plain(BufWriter<TcpStream>)` / `Tls(Box<BufWriter<TlsStream<TcpStream>>>)`) を追加し、`AsyncRead` / `AsyncWrite` を delegation で実装した。TLS バリアントは `large_enum_variant` を避けるため `Box` で wrap する
- `PooledConnection` の `stream` フィールドを `UpstreamStream` に変更した
- `ConnectionPool` の `idle_connections` キーを `(Scheme, String, u16)` のタプル (`UpstreamKey`) に変更し、`try_acquire` / `release` のシグネチャも揃えた
- `create_connection(scheme, host, port, tls_connector)` で scheme 分岐させ、`Scheme::Http` は plaintext、`Scheme::Https` は SNI に裸の host (ブラケット無し) を渡して TLS 接続する
- `stream_response_on_connection` の `upstream` 引数を `&mut UpstreamStream` に変更し、`BufWriter` 内蔵のため `write_all` 後に `flush` を呼ぶよう変更した
- `--upstream` の help text を `Examples: http://internal:8080, https://[::1]:8443` まで明記するよう更新した
- `CHANGES.md` の `## develop` に `[FIX]` エントリを追加した
