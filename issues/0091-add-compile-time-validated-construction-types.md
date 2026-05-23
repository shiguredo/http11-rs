# HeaderName / Method / Scheme にコンパイル時検査つき構築型を導入する

- Priority: Medium
- Created: 2026-05-23
- Model: Opus 4.7

## 目的

HTTP/1.1 のヘッダー名・メソッド・URI スキームは、利用者コードでリテラル定数として
書かれることが圧倒的に多い。これらにランタイム検査つきの構築 API
(`new() -> Result`) と、リテラル定数向けの `const fn from_static` を併設し、
**RFC 違反のリテラルをコンパイル時に検出可能**にする。

これは shiguredo_http11 の差別化要素となり、他の HTTP/1.1 ライブラリには無い特徴。

## 優先度根拠

Medium。

- High ではない理由: 既存の `Request::header(name, value)` / `Request::new(method, target)`
  はランタイム検査 (`Result<Self, EncodeError>`) を既に行っており、本質的な安全性は
  確保されている。本 issue は「リテラル定数のミスをコンパイル時に検出する」上乗せ
- Low ではない理由: 「リテラル定数で書かれるヘッダー名・メソッドの RFC 違反を
  コンパイル時に検出できる」差別化を打ち出すために必要。後発で導入するほど破壊的変更
  コストが膨らむ

## 現状

現状の構築 API はすべてランタイム検査:

- `Request::new(method: &str, target: &str) -> Result<Self, EncodeError>` (`src/request.rs`)
- `Request::header(self, name: &str, value: &str) -> Result<Self, EncodeError>` (`src/request.rs`)
- `Response::header` も同様

問題点:

- `Request::header("Host", "example.com")` のように、リテラルでも `?` が必要
  (大文字 "Host" は許容されているが、CRLF を含むリテラル等のミスもランタイムでしか検出できない)
- 利用者が同じヘッダー名リテラルを複数箇所で書く際、タイプミスがランタイムまで露見しない
- 「他のライブラリにない、コンパイル時に RFC 違反に気付ける」差別化を打ち出していない

## 設計方針

### 新規型

```rust
// src/header_name.rs (新規)
/// HTTP ヘッダー名 (RFC 9110 §5.1)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HeaderName(Cow<'static, [u8]>);

#[non_exhaustive]
pub enum HeaderNameError {
    Empty,
    InvalidByte { byte: u8, position: usize },
}

impl HeaderName {
    /// ランタイム値から検査つきで構築する
    pub fn new(name: impl AsRef<[u8]>) -> Result<Self, HeaderNameError>;

    /// 静的バイト列から検査つきで構築する (const fn)
    /// 不正リテラルはコンパイル時 panic (= コンパイルエラー)
    pub const fn from_static(name: &'static [u8]) -> Self;

    pub fn as_bytes(&self) -> &[u8];

    /// 検査済みバイト列から検査をスキップして構築する (decoder 専用)
    pub(crate) fn from_validated_parts(name: Cow<'static, [u8]>) -> Self;
}

// src/method.rs (新規)
/// HTTP メソッド (RFC 9110 §9)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Method(Cow<'static, [u8]>);

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

    /// 標準メソッド定数 (全て const)
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

// src/uri.rs に追加 (新規ファイルではなく既存 uri.rs に追加)
/// URI スキーム (RFC 3986 §3.1)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Scheme(Cow<'static, [u8]>);

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

破壊的変更前提。`&str` を受ける引数を新型に置き換える。

```rust
// 変更前
impl Request {
    pub fn new(method: &str, target: &str) -> Result<Self, EncodeError>;
    pub fn header(self, name: &str, value: &str) -> Result<Self, EncodeError>;
}

// 変更後
impl Request {
    pub fn new(method: Method, target: RequestTarget) -> Self;
    pub fn header(self, name: HeaderName, value: impl AsRef<[u8]>) -> Result<Self, EncodeError>;
}
```

検査済みの `Method` / `HeaderName` を受け取るため、`Request::new` 自体は
`Result` を返す必要がなくなる。`header` の `value` は値の RFC 制約上ランタイム検査が必要なため
`Result` を維持する。

### コンパイル時検査の例

```rust
// OK
const HOST: HeaderName = HeaderName::from_static(b"host");
const REQ_LINE_METHOD: Method = Method::GET;

// NG: コンパイル時に "header name byte 'H' (0x48) at position 0 is uppercase" で fail
const BAD_NAME: HeaderName = HeaderName::from_static(b"Host");

// NG: コンパイル時に "header name contains CR at position 4" で fail
const INJECT: HeaderName = HeaderName::from_static(b"host\r\nX-Inject");

// NG: コンパイル時に "method contains lowercase byte at position 0" で fail
const BAD_METHOD: Method = Method::from_static(b"get");
```

### `const fn` の制約への対応

http11 は `#![cfg_attr(not(test), no_std)]` で `alloc::string::String` / `alloc::vec::Vec`
を使うが、`const fn` 内で `Vec`/`String` を生成することはできない。

対応:
- 新型は `Cow<'static, [u8]>` を内部表現にする
  - `from_static` 経路: `Cow::Borrowed(&'static [u8])` (`const fn` で生成可能)
  - `new` 経路: `Cow::Owned(Vec::from(bytes))` (ランタイム)
- `const fn` 内では `panic!` で fail。MSRV 1.88 で `const Try` (`?`) は未安定だが
  `panic!` は const 文脈で動作する
- `const fn` 内の検査ロジックは while ループで bytes を走査 (for は const 不可)

### Cargo features の検討

`const fn from_static` を使う利用者は `core::panic` で fail するため、
ライブラリ側の追加 feature は不要。ただし `trybuild` ベースのテストは
`#[cfg(test)]` 配下に置く (別 issue で対応)。

## 完了条件

- `src/header_name.rs` / `src/method.rs` が新設され、`HeaderName` / `Method` が
  `new` (Result) と `from_static` (const fn) を提供している
- `src/uri.rs` に `Scheme` 型が追加され、同様の API を持つ
- `Method` / `Scheme` に標準値の `const` 定数 (GET/POST/.../HTTP/HTTPS/...) が提供されている
- `Request::new` / `Request::header` / `Response::header` / `Response::with_status` 等の
  公開 API が新型を受け取るように変更されている
- decoder 経路は `from_validated_parts` 経由で構築している
- 不正なリテラルを `from_static` に渡したサンプルが doc 内で `compile_fail` 属性で示されている
- `CHANGES.md` の `## develop` に `[CHANGE]` と `[ADD]` のエントリが追加されている
- 既存の全テスト・PBT・fuzz が通る
- 既存 examples が新 API に追従して動作する

## 解決方法

実装順:

1. `src/header_name.rs` 新設 (`HeaderName` + エラー型 + `const fn from_static`)
2. `src/method.rs` 新設 (`Method` + エラー型 + `const fn from_static` + 標準定数)
3. `src/uri.rs` に `Scheme` 追加 (同上)
4. `src/decoder/` で `from_validated_parts` 経由で構築するよう書き換え
5. `src/request.rs` / `src/response.rs` の公開 API を新型に置き換え
6. `examples/`, `tests/`, `pbt/`, `fuzz/` を新 API に追従

PBT による整合性検証 (`from_static` と `new` の結果一致、decoder と `new` の受理集合一致) は
別 issue で対応する。

## 関連

- 本 issue 完了後に 0092 (trybuild) / 0093 (PBT 整合性) を着手する
