# HeaderName / Method / Scheme にコンパイル時検査つき構築型を導入する

- Priority: Medium
- Created: 2026-05-23
- Completed: 2026-05-24
- Model: deepseek v4-pro
- Branch: feature/add-compile-time-validated-types

## 目的

HTTP/1.1 のヘッダー名・メソッド・URI スキームは、RFC 9110 上すべて `token = 1*tchar`（または `scheme = ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )`）の構文を持つ。現在の API は `&str` / `impl Into<String>` でこれらを受け取り、ランタイム検査（`is_valid_header_name` / `is_valid_method`）を `EncodeError` 経由で返している。

これらに専用の構築型を導入し、`new() -> Result` と `const fn from_static` の二経路を提供することで、リテラル定数の不正（CR / LF / NUL 混入等）をコンパイル時に検出可能にする。

## RFC 準拠の文字種ポリシー

RFC 9110 Section 5.6.2 の定義:

```
token = 1*tchar
tchar = "!" / "#" / "$" / "%" / "&" / "'" / "*"
        / "+" / "-" / "." / "^" / "_" / "`" / "|" / "~"
        / DIGIT / ALPHA
```

`ALPHA = %x41-5A / %x61-7A`（大文字 A-Z と小文字 a-z の両方を含む）。

各型の文字種ポリシーは RFC の ABNF 文法に基づき、以下のとおりとする:

| 型 | ABNF | 受理する文字 | 内部正規化 | Eq/Hash |
|---|---|---|---|---|
| `HeaderName` | `field-name = token` | 全 tchar（大文字含む） | 小文字化 | case-insensitive |
| `Method` | `method = token` | 全 tchar（小文字含む） | なし（case-sensitive） | case-sensitive |
| `Scheme` | `scheme = ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )` | 全 scheme 文字（大文字含む） | 小文字化 | case-insensitive |

- `HeaderName`: RFC 9110 Section 5.1 "Field names are case-insensitive" に従い、case-insensitive な `Eq` / `Hash` を手動実装する。大文字を含む入力（`b"Host"`）は受理し、そのまま保持する（`const fn` では borrowed bytes を変更できないため）。`as_bytes()` は格納されたバイト列をそのまま返す（正規化済みとは限らない）。
- `Method`: RFC 9110 Section 9.1 "The method token is case-sensitive" に従い、case-preserving で保持し `derive(Eq, Hash)` を使用する。標準メソッドの const 定数は convention に従い大文字で提供するが、`from_static(b"get")` も受理する（あくまで `b"GET"` とは異なるメソッドとして扱われる）。
- `Scheme`: RFC 3986 Section 3.1 "should accept uppercase letters as equivalent to lowercase" に従い、case-insensitive な `Eq` / `Hash` を手動実装する。大文字を含む入力は受理しそのまま保持する。

## 優先度根拠

Medium。
- High ではない理由: 既存のランタイム検査で安全性は確保されている。本 issue は「リテラル定数ミスのコンパイル時検出」の上乗せ。
- Low ではない理由: 後発で導入するほど破壊的変更コストが膨らむ。また「リテラル定数で書かれるヘッダー名・メソッドの RFC 違反をコンパイル時に検出できる」点は差別化要素となる。

## 現状

- `Request::new(method: impl Into<String>, uri: impl Into<String>) -> Result<Self, EncodeError>` (`src/request.rs:73`)
- `Request::header(name: impl Into<String>, value: impl Into<String>) -> Result<Self, EncodeError>` (`src/request.rs:213`)
- `Request::add_header(&mut self, name: impl Into<String>, value: impl Into<String>)` (`src/request.rs:265`)
- `Request::set_header(&mut self, name: impl Into<String>, value: impl Into<String>)` (`src/request.rs:298`)
- `Response::header(name: impl Into<String>, value: impl Into<String>)` (`src/response.rs`)
- `Response::add_header(&mut self, name: impl Into<String>, value: impl Into<String>)` (`src/response.rs`)
- `Response::set_header(&mut self, name: impl Into<String>, value: impl Into<String>)` (`src/response.rs`)
- `RequestHead::new(method: &str, uri: &str)`, `RequestHead::header(name: &str, value: &str)` (`src/decoder/head.rs:187,215`)
- `ResponseHead::header(name: &str, value: &str)` (`src/decoder/head.rs:377`)
- `HttpHead::headers()` の戻り型は `&[(String, String)]`（trait、`src/decoder/head.rs:17`）

## 設計方針

### 新規型

```rust
// src/header_name.rs (新規)
/// HTTP ヘッダー名 (RFC 9110 Section 5.1, field-name = token)
///
/// Eq/Hash は case-insensitive（手動実装）。
/// `const fn from_static` では borrowed bytes を変更できないため、
/// 内部正規化を行わずに保持し、比較時に case-insensitive 判定を行う。
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct HeaderName(Cow<'static, [u8]>);

// PartialEq, Eq, Hash は case-insensitive な手動実装

#[derive(Debug)]
#[non_exhaustive]
pub enum HeaderNameError {
    Empty,
    InvalidByte { byte: u8, position: usize },
}

impl HeaderName {
    pub fn new(name: impl AsRef<[u8]>) -> Result<Self, HeaderNameError>;
    pub const fn from_static(name: &'static [u8]) -> Self;
    pub fn as_bytes(&self) -> &[u8];
    pub(crate) fn from_validated_parts(name: Cow<'static, [u8]>) -> Self;
}

// src/method.rs (新規)
/// HTTP メソッド (RFC 9110 Section 9.1, method = token)
///
/// case-sensitive。Eq/Hash も case-sensitive。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Method(Cow<'static, [u8]>);

#[derive(Debug)]
#[non_exhaustive]
pub enum MethodError {
    Empty,
    InvalidByte { byte: u8, position: usize },
}

impl Method {
    pub fn new(method: impl AsRef<[u8]>) -> Result<Self, MethodError>;
    pub const fn from_static(method: &'static [u8]) -> Self;
    pub fn as_bytes(&self) -> &[u8];
    pub(crate) fn from_validated_parts(method: Cow<'static, [u8]>) -> Self;

    pub const GET: Self = Self::from_static(b"GET");
    pub const POST: Self = Self::from_static(b"POST");
    pub const PUT: Self = Self::from_static(b"PUT");
    pub const DELETE: Self = Self::from_static(b"DELETE");
    pub const HEAD: Self = Self::from_static(b"HEAD");
    pub const OPTIONS: Self = Self::from_static(b"OPTIONS");
    pub const CONNECT: Self = Self::from_static(b"CONNECT");
    pub const TRACE: Self = Self::from_static(b"TRACE");
    pub const PATCH: Self = Self::from_static(b"PATCH");
}

// src/uri.rs に追加
/// URI スキーム (RFC 3986 Section 3.1)
///
/// Eq/Hash は case-insensitive（手動実装）。
/// `const fn from_static` では borrowed bytes を変更できないため、
/// 内部正規化を行わずに保持し、比較時に case-insensitive 判定を行う。
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Scheme(Cow<'static, [u8]>);

// PartialEq, Eq, Hash は case-insensitive な手動実装

#[derive(Debug)]
#[non_exhaustive]
pub enum SchemeError {
    Empty,
    InvalidFirstByte { byte: u8 },
    InvalidByte { byte: u8, position: usize },
}

impl Scheme {
    pub fn new(scheme: impl AsRef<[u8]>) -> Result<Self, SchemeError>;
    pub const fn from_static(scheme: &'static [u8]) -> Self;
    pub fn as_bytes(&self) -> &[u8];
    pub(crate) fn from_validated_parts(scheme: Cow<'static, [u8]>) -> Self;

    pub const HTTP: Self = Self::from_static(b"http");
    pub const HTTPS: Self = Self::from_static(b"https");
    pub const WS: Self = Self::from_static(b"ws");
    pub const WSS: Self = Self::from_static(b"wss");
    pub const RTSP: Self = Self::from_static(b"rtsp");
}
```

### 既存 API の書き換え

`name: impl Into<String>` の引数を `name: HeaderName` に、`method: impl Into<String>` を `method: Method` に置き換える。

```rust
// 変更前
impl Request {
    pub fn new(method: impl Into<String>, uri: impl Into<String>) -> Result<Self, EncodeError>;
    pub fn header(self, name: impl Into<String>, value: impl Into<String>) -> Result<Self, EncodeError>;
}

// 変更後
impl Request {
    pub fn new(method: Method, uri: impl Into<String>) -> Result<Self, EncodeError>;
    pub fn header(self, name: HeaderName, value: impl Into<String>) -> Result<Self, EncodeError>;
}
```

- `Request::new` の `uri` は `String` のまま維持する（`RequestTarget` 型は本 issue のスコープ外。別 issue で対応する）
- `header` の `value` 引数は `impl Into<String>` を維持する（`value` の search には `&str` のままの方が内部実装との整合性が取れる）
- `add_header` / `set_header` も同様に変更する
- `RequestHead` / `ResponseHead` の対応する API も全て同様に変更する（`src/decoder/head.rs`）
- `HttpHead::headers()` の戻り型は `&[(HeaderName, String)]` に変更する（破壊的変更。後方互換なしの `[CHANGE]` に分類）

### コンパイル時検査の例

```rust
// OK: コンパイル時に受理される正当なリテラル
const HOST: HeaderName = HeaderName::from_static(b"host");    // 小文字
const HOST_UC: HeaderName = HeaderName::from_static(b"Host"); // 大文字、内部で小文字正規化
const GET: Method = Method::from_static(b"GET");
const CUSTOM: Method = Method::from_static(b"WebDAV-MOVE");   // 拡張メソッド
const HTTP: Scheme = Scheme::from_static(b"http");
const HTTPS_UC: Scheme = Scheme::from_static(b"HTTPS");       // 大文字、内部で小文字正規化

// NG: コンパイル時に panic
// HeaderName: 空 / CR / LF / NUL / 区切り文字 ( ) , / : ; < = > ? @ [ \ ] { } / DQUOTE
const EMPTY_NAME: HeaderName = HeaderName::from_static(b"");
const CR_INJECT: HeaderName = HeaderName::from_static(b"host\r\nX-Inject: evil");
// Method: 空 / CR / LF / NUL / 区切り文字
const EMPTY_METHOD: Method = Method::from_static(b"");
const CR_METHOD: Method = Method::from_static(b"GET\r");
// Scheme: 空 / 数字開始 / 不正文字
const DIGIT_START: Scheme = Scheme::from_static(b"3http");
const COLON_SCHEME: Scheme = Scheme::from_static(b"http:");
```

### `const fn` の制約への対応

- 内部表現は `Cow<'static, [u8]>` を使用。`from_static` は `Cow::Borrowed`、`new` は `Cow::Owned`（ランタイム）
- `const fn` 内の検査は `while` ループでバイト走査（`for` は const 不可）
- エラーは `panic!`（const 文脈で動作）。MSRV 1.88 で `const Try` (`?`) は未安定だが `panic!` は const 可能
- `HeaderName` / `Scheme` の case-insensitive `Eq` / `Hash` は手動実装する。`const fn from_static` は borrowed bytes を変更できないため、正規化は構築時ではなく比較時に行う（`PartialEq::eq` 内で `eq_ignore_ascii_case` を使用）

### `HttpHead` トレイトの変更

`HttpHead::headers()` の戻り型を `&[(String, String)]` から `&[(HeaderName, String)]` に変更する。

- これは `HttpHead` を実装するすべての型（`Request`, `Response`, `RequestHead`, `ResponseHead`）に波及する破壊的変更
- トレイトの各メソッドで `name.eq_ignore_ascii_case(...)` していた箇所は、`HeaderName` の case-insensitive `Eq` を利用できるようになる
- `encoder.rs` の `name.as_bytes()` は `HeaderName::as_bytes()` に置き換える

### decoder 経路

decoder はパース時に `String` として保持しているヘッダー名・メソッドを、`from_validated_parts` 経由で `Cow::Owned(Vec<u8>)` に変換して渡す。

- `decoder/head.rs` の `RequestHead` / `ResponseHead` の内部フィールドを `String` から `HeaderName` / `Method` に変更する
- `RequestHead::method` (`String`) → `Method`
- `RequestHead::headers` (`Vec<(String, String)>`) → `Vec<(HeaderName, String)>`
- decoder で `parts[0].to_string().into_bytes()` → `Cow::Owned` → `Method::from_validated_parts`
- 追加アロケーションは発生するが、decoder 経路は既に `to_string()` でアロケーションしているため差分は `String → Vec<u8>` の 1 回のみ

## 完了条件

- `src/header_name.rs` / `src/method.rs` が新設され、各型が `new` (Result) と `from_static` (const fn) を提供している
- `src/uri.rs` に `Scheme` 型が追加され、同様の API を持つ
- `Method` / `Scheme` に標準値の `const` 定数が提供されている
- 以下の公開 API が新型を受け取るように変更されている:
  - `Request::new`, `header`, `add_header`, `set_header`
  - `Response::header`, `add_header`, `set_header`
  - `RequestHead::new`, `with_version`, `header`, `add_header`
  - `ResponseHead::header`, `add_header`
- `HttpHead::headers()` の戻り型が `&[(HeaderName, String)]` に変更されている
- decoder 経路が `from_validated_parts` 経由で構築している
- encoder が `HeaderName::as_bytes()` を使用している
- `pub(crate) mod header_name` / `pub(crate) mod method` で始め、安定後に `pub mod` に昇格させるか否かは別途判断する
- `CHANGES.md` の `## develop` に `[CHANGE]`（HttpHead 戻り型変更、API 破壊）と `[ADD]`（新規型 + `from_static`）のエントリが追加されている
- 既存の全テスト・PBT・fuzz が通る
- examples が新 API に追従して動作する

## 解決方法

実装順:

1. `src/header_name.rs` 新設（`HeaderName` + エラー型 + `const fn from_static`）
2. `src/method.rs` 新設（`Method` + エラー型 + `const fn from_static` + 標準定数）
3. `src/uri.rs` に `Scheme` 追加
4. `src/decoder/head.rs` の `RequestHead` / `ResponseHead` の内部フィールド型変更
5. `src/decoder/` で `from_validated_parts` 経由の構築に書き換え
6. `HttpHead::headers()` の戻り型変更と全実装の追従
7. `src/request.rs` / `src/response.rs` の公開 API を新型に置き換え
8. `src/encoder.rs` を `HeaderName::as_bytes()` に書き換え
9. `examples/`, `tests/`, `pbt/`, `fuzz/` を新 API に追従

PBT による整合性検証（`from_static` と `new` の結果一致、decoder と `new` の受理集合一致）は別 issue（0093）で対応する。

## 関連

- 本 issue 完了後に 0092（trybuild コンパイル時エラーテスト）と 0093（PBT 整合性検証）を着手する
- `RequestTarget` 型の導入は本 issue のスコープ外（`uri` は `impl Into<String>` のまま維持する）

## 解決方法

### 変更ファイル

#### 新規ファイル
- `src/header_name.rs`: `HeaderName` 型 (Cow<'static, [u8]>, case-insensitive Eq/Hash, const fn from_static)
- `src/method.rs`: `Method` 型 (Cow<'static, [u8]>, case-sensitive, const fn from_static, 標準定数 GET/POST/...)

#### コアライブラリ (src/)
- `src/lib.rs`: `header_name`, `method` モジュール追加、`HeaderName/HeaderNameError/Method/MethodError/Scheme/SchemeError` を re-export
- `src/uri.rs`: `Scheme` 型追加 (Cow<'static, [u8]>, case-insensitive Eq/Hash, const fn from_static, 標準定数 HTTP/HTTPS/WS/WSS/RTSP)
- `src/decoder/head.rs`: `HttpHead::headers()` 戻り型を `&[(HeaderName, String)]` に変更、`RequestHead`/`ResponseHead` の内部フィールドを `Method`/`HeaderName` に変更
- `src/request.rs`: `method: Method` / `headers: Vec<(HeaderName, String)>` に変更、公開 API シグネチャを `Method`/`HeaderName` に変更
- `src/response.rs`: `headers: Vec<(HeaderName, String)>` に変更、公開 API シグネチャを `HeaderName` に変更
- `src/encoder.rs`: `validate_headers` シグネチャ変更、`name.as_bytes().len()` / `name == "..."` に更新
- `src/decoder/request.rs`: 内部ヘッダー格納を `HeaderName::from_validated_bytes` 経由に変更
- `src/decoder/response.rs`: 同上
- `src/decoder/body.rs`: `parse_content_length` 等のシグネチャ変更、trailer/trailers を `HeaderName` に変更

#### テスト・PBT・Fuzz (全ファイル)
- 全テスト (~500 call site) を新 API に追従
- `Request::new("GET", ...)` → `Request::new(Method::GET, ...)`
- `.header("Name", ...)` → `.header(HeaderName::from_static(b"Name"), ...)`
- 不正値テストは `Method::new()` / `HeaderName::new()` のエラー検査に変更
- PBT の strategy を `Method` / `HeaderName` 型に変更

#### Examples
- `examples/http11_server/src/main.rs`: 新 API 追従
- `examples/http11_reverse_proxy/src/main.rs`: 新 API 追従、`is_hop_by_hop_header` シグネチャ変更
- `examples/http11_client/src/main.rs`, `transport.rs`: 新 API 追従
- `examples/http11_server_io_uring/src/main.rs`: 新 API 追従
- `examples/http11_client/tests/`: 新 API 追従

### テスト

- 全ワークスペーステスト通過確認
- Clippy (-D warnings) 通過確認
- cargo fmt 通過確認
- http11_server E2E テスト (26 件) 通過確認
