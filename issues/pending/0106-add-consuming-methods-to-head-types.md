# RequestHead / ResponseHead に消費メソッドを追加する

Created: 2026-06-06
Priority: Medium
Model: deepseek-v4-pro
Polished: 2026-06-06
Pending: 2026-06-06

## pending 理由

実用上のパフォーマンス改善は誤差レベル。クローン対象は数バイトの ASCII 文字列であり、既存の回避策 (`Method::new()` / `HeaderName::new()`) で十分動作する。API 設計の美学の問題であり、優先度を下げて保留とする。

## 目的

`RequestHead` / `ResponseHead` は `Method` / `HeaderName` を所有しているが、公開 API では `&str` の参照しか返さない。このため、ストリーミングデコーダーの利用者（リバースプロキシ等）が `RequestHead` から新しい `Request` を構築する際に、すでに所有しているメソッドやヘッダー名を再アロケーション＋再検証している。デコーダー内部 (`from_validated_parts` / `from_raw_parts`) はムーブでゼロコピーできているのに、外部利用者にはそれができない。

消費メソッド (`into_xxx`) を追加し、所有権の移譲を可能にする。

## 優先度根拠

- 既存の回避策 (`Method::new()` / `HeaderName::new()`) は存在する
- 実用上のパフォーマンス影響は軽微（メソッド名・ヘッダー名は数バイト〜数十バイト）
- 「所有している値を取り出せないまま再クローンする」のは API 設計として不自然であり、Sans I/O のゼロコピー設計思想とも整合しない
- 中程度の問題として Medium。パフォーマンス改善が主目的ではなく API 設計の改善のため、AGENTS.md 冒頭の "Premature Optimization is the Root of All Evil" には抵触しない

## 現状

### RequestHead

```rust
// src/decoder/head.rs:170-179
pub struct RequestHead {
    pub(crate) method: Method,       // 所有
    pub(crate) uri: String,          // 所有
    pub(crate) version: String,      // 所有
    pub(crate) headers: Vec<(HeaderName, String)>,  // 所有
}
```

公開 getter はすべて参照を返す:

```rust
// src/decoder/head.rs:246-269
pub fn method(&self) -> &str { self.method.as_str() }
pub fn uri(&self) -> &str { &self.uri }
pub fn version(&self) -> &str { &self.version }
pub fn headers(&self) -> &[(HeaderName, String)] { &self.headers }
```

デコーダー内部では `from_validated_parts` (pub(crate)) 経由で所有権を `Request::from_raw_parts` に直接渡し、ゼロコピーで `Request` を構築している。

### reverse proxy の現状の回避策

```rust
// examples/http11_reverse_proxy/src/main.rs:641
Method::new(req_head.method().as_bytes()).expect("decoder-validated method")
// → Method を再アロケーション。デコーダーがすでに Method を所有しているのに、
//   一旦 &str として取り出し、再度 bytes に変換して new() で再構築している。

// examples/http11_reverse_proxy/src/main.rs:672
upstream_request.add_header(name.clone(), value)?;
// → HeaderName を clone (Cow::Owned → Cow::Owned の再アロケーション)
```

### ResponseHead

```rust
// src/decoder/head.rs:329-338
pub struct ResponseHead {
    pub(crate) version: String,
    pub(crate) status_code: u16,
    pub(crate) reason_phrase: String,
    pub(crate) headers: Vec<(HeaderName, String)>,
}
```

`RequestHead` と同じ構造。`headers` の `HeaderName` がムーブできず、再構築時に同様の問題が発生する。

## 設計方針

### 追加する API

すべての `into_xxx()` は `self` を消費する。同一インスタンスから複数の消費メソッドを呼ぶことはできない。全フィールドを一度に取り出すには `into_parts()` を使う。

#### RequestHead

```rust
impl RequestHead {
    /// 所有している Method を消費して取り出す
    pub fn into_method(self) -> Method;

    /// 所有している URI を消費して取り出す
    pub fn into_uri(self) -> String;

    /// 所有している HTTP バージョンを消費して取り出す
    pub fn into_version(self) -> String;

    /// 所有しているヘッダーを消費して取り出す
    pub fn into_headers(self) -> Vec<(HeaderName, String)>;

    /// 全フィールドを消費してタプルで取り出す
    /// 戻り値: (method: Method, uri: String, version: String, headers: Vec<(HeaderName, String)>)
    pub fn into_parts(self) -> (Method, String, String, Vec<(HeaderName, String)>);
}
```

#### ResponseHead

```rust
impl ResponseHead {
    /// 所有している HTTP バージョンを消費して取り出す
    pub fn into_version(self) -> String;

    /// 所有している reason-phrase を消費して取り出す
    pub fn into_reason_phrase(self) -> String;

    /// 所有しているヘッダーを消費して取り出す
    pub fn into_headers(self) -> Vec<(HeaderName, String)>;

    /// 全フィールドを消費してタプルで取り出す
    /// 戻り値: (version: String, status_code: u16, reason_phrase: String, headers: Vec<(HeaderName, String)>)
    pub fn into_parts(self) -> (String, u16, String, Vec<(HeaderName, String)>);
}
```

### `into_parts()` から取り出した値の受け渡し

`into_parts()` が返す `Method` は標準ライブラリの blanket impl (`From<T> for T` → `TryFrom<T> for T`) により `Request::with_version()` の `impl TryInto<Method>` にそのまま渡せる。`HeaderName` も同様に `Request::add_header()` / `Response::add_header()` の `impl TryInto<HeaderName>` に渡せる。

### reverse proxy での利用イメージ

#### RequestHead → upstream Request

変更前:

```rust
let mut upstream_request = Request::new(
    Method::new(req_head.method().as_bytes()).expect("decoder-validated method"),
    req_head.uri(),
)?;

for (name, value) in req_head.headers() {
    // ...
    upstream_request.add_header(name.clone(), value)?;
}
```

変更後:

```rust
let (method, uri, version, headers) = req_head.into_parts();
let mut upstream_request = Request::with_version(method, uri, version)?;

for (name, value) in headers {
    // ...
    upstream_request.add_header(name, value)?;
}
```

変更後はクライアントの HTTP バージョン（例: `HTTP/1.0`）がそのまま upstream に転送される。これは RFC 9112 Section 2.3（proxy は受信した HTTP-version を転送先メッセージの version として転送 MUST。リクエスト転送時は request-line version、レスポンス転送時は status-line version）に準拠する正しい動作変更である。

#### ResponseHead → client Response

変更前:

```rust
let mut response = Response::new(head.status_code(), head.reason_phrase())?;

for (name, value) in head.headers() {
    if is_hop_by_hop_header(name, &connection_headers) {
        continue;
    }
    response = response.header(name.clone(), value)?;
}
```

変更後:

```rust
let (version, status_code, reason_phrase, headers) = head.into_parts();
let mut response = Response::with_version(version, status_code, reason_phrase)?;

for (name, value) in headers {
    if is_hop_by_hop_header(&name, &connection_headers) {
        continue;
    }
    response = response.header(name, value)?;
}
```

### 制限事項

#### obs-text を含む URI は `with_version()` で失敗する

`RequestHead::from_validated_parts` は受信側互換性のため obs-text (0x80-0xFF) を含む URI を許容するが、`Request::with_version` は送信側ポリシーとして obs-text を拒否する (`src/request.rs:123`)。このため、obs-text を含む URI を `into_parts()` で取り出して `with_version()` に渡すと `EncodeError::InvalidRequestTarget` が返る。これは現行の `Request::new(req_head.uri())` でも同様に発生する制限であり、本 issue のスコープ外とする。PBT では obs-text を含まない strategy を使用する。

#### 空 reason-phrase は `Response::with_version()` で失敗する

`ResponseHead` は受信側として reason-phrase の absent（空文字列）を許容する（RFC 9112 Section 4）が、`Response::with_version` は送信側として空 reason-phrase を拒否する（`src/response.rs:157`）。このため、空 reason-phrase の `ResponseHead` から `into_parts()` で取り出した値を `Response::with_version` に渡すと `EncodeError::InvalidReasonPhrase` が返る。これは現行の `Response::new(head.status_code(), head.reason_phrase())` でも同様に発生する制限であり、本 issue のスコープ外とする。PBT では空 reason-phrase を除外する strategy を使用する。

#### `with_version()` / `add_header()` は再検証を行う

`Request::with_version()` は URI / version の構文検証を、`add_header()` は header value の構文検証をそれぞれ実行する。デコーダー側で検証済みの値に対して冗長だが、公開 API の防御線として意図的に残す。アロケーション（`String` / `Vec` の複製）はムーブにより回避される。

### `into_parts()` と `#[non_exhaustive]` の関係

`RequestHead` / `ResponseHead` には `#[non_exhaustive]` が付与されており、将来のフィールド追加時に構造体リテラルでの外部構築や網羅的パターンマッチが破壊されるのを防いでいる。`into_parts()` は戻り型がタプルであるため、将来フィールドが追加された場合にタプルの要素数が変わり、呼び出し側がコンパイルエラーになる。これは許容する。プロジェクト規約上、破壊的変更は許容されており、将来のフィールド追加時には `into_parts()` の戻り型変更を破壊的変更として扱う。個別の `into_xxx()` は新フィールド追加時にも影響を受けないため、安定した API が必要な場合は個別メソッドを使う。

### 検討したが不採用の方針

#### A. `TryFrom<&str>` を `Method` / `HeaderName` に追加する

- `RequestHead` がすでに所有している値を `&str` 経由で再構築するのは無駄なアロケーションを隠蔽するだけであり、本質的な問題（`RequestHead` が所有値を外部に出せないこと）を解決しない

#### B. フィールドを `pub` にする

- カプセル化を破壊する
- すでに `#[non_exhaustive]` が付与されているため、将来のフィールド追加との両立が難しい

#### C. `From<RequestHead> for Request` を実装する

- `Request::new()` / `with_version()` が `Result` を返す（エンコード時の意味論チェックが失敗しうる）ため、`From` のシグネチャと整合しない
- `RequestHead` から `Request` への変換は単純なムーブでは完結せず、Host ヘッダーの追加等が必要

## 完了条件

### 実装

- `RequestHead` に `into_method()`, `into_uri()`, `into_version()`, `into_headers()`, `into_parts()` を追加する (`src/decoder/head.rs`)
- `ResponseHead` に `into_version()`, `into_reason_phrase()`, `into_headers()`, `into_parts()` を追加する (`src/decoder/head.rs`)
- 既存の参照 getter (`method()`, `uri()`, `version()`, `headers()`, `status_code()`, `reason_phrase()`) は変更しない（後方互換）

### テスト

- **PBT**: `pbt/tests/prop_decoder/head.rs` に以下を追加する
  - `RequestHead` roundtrip: `into_parts()` → `Request::with_version()` → `encode_headers()` → デコード → 元の `RequestHead` と method / uri / version / headers が一致する。strategy は obs-text を含まない URI に制限する
  - `ResponseHead` roundtrip: `into_parts()` → `Response::with_version()` → `encode_headers()` → デコード → 元の `ResponseHead` と version / status_code / reason_phrase / headers が一致する。strategy は空 reason-phrase を除外する
  - 個別 `into_xxx()` メソッドの roundtrip (各フィールドを取り出して再構築し一致を検証)
- **単体テスト**: `tests/test_decoder/head.rs` に obs-text URI のエラーパスを追加する（`into_parts()` → `with_version()` が obs-text URI で `Err` を返すことの検証）
- **fuzzing**: `fuzz/fuzz_targets/fuzz_decoder_head_into_parts.rs` を新設し、任意入力 → デコード → `into_parts()` → `with_version()` のラウンドトリップでパニックしないことを検証する（fuzz クレートは別 crate のため `pub(crate)` な `from_raw_parts` は使用不可）
- `cargo test --workspace --all-targets` がすべて PASS すること
- `cargo clippy --workspace --all-targets -- -D warnings` が PASS すること
- `cargo fmt --all -- --check` が PASS すること

### ドキュメント・サンプル

- `examples/http11_reverse_proxy/src/main.rs` を `into_parts()` を使う形に更新する（`RequestHead` / `ResponseHead` の両方）
- `CHANGES.md` の `## develop` に `[ADD]` エントリを追加する
  ```
  - [ADD] RequestHead / ResponseHead に所有権を消費してフィールドを取り出す into_xxx() メソッドと into_parts() メソッドを追加する
    - @voluntas
  ```
