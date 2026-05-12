# 0054: 公開 enum に `#[non_exhaustive]` を一括付与しバリアント追加を非破壊化する

Created: 2026-05-13
Model: Opus 4.7

## 概要

本クレートの公開 enum の大半は、将来バリアントが追加される実態がある。特に Error enum は本リリース develop 区間だけでも以下のバリアントを追加している:

- `EncodeError::DuplicateContentLength` / `InvalidContentLengthValue`
- `AuthError::ConflictingUsernameField` / `InvalidUsernameExtValue` / `TooManyParameters`
- `ContentDispositionError::TooManyParameters`

また `BodyKind` は本リリースで `Tunnel` バリアントを追加 (CHANGES.md L114-120) しており、`StatusClass` は新規導入 (CHANGES.md L170-175) で IANA レジストリ拡張に追従する余地がある。

これら公開 enum に `#[non_exhaustive]` を付与せずにリリースすると、**次回のバリアント追加ごとに後方互換のない変更を出し続ける構造** になる。今リリースで一括付与しないと、付与自体が将来の breaking change になる。

```rust
// 影響例 (利用側)
match err {
    EncodeError::DuplicateContentLength => ...,
    EncodeError::InvalidContentLengthValue => ...,
    // ... 全 variant を網羅
}
// バリアント追加で利用側がコンパイルエラーになる (= 破壊的変更)
```

`#[non_exhaustive]` を付与すると、利用側は `_ =>` wildcard arm を強制され、バリアント追加が非破壊化される。

## 根拠

### Rust API ガイドライン (C-STABLE)

- `clippy::exhaustive_enums` が error enum への `#[non_exhaustive]` 付与を推奨。
- `std::io::ErrorKind` / `std::num::IntErrorKind` 等の標準 error enum も `#[non_exhaustive]` 付与で運用されている。

### CHANGES.md の整合性

CHANGES.md L106-108 では `RequestHead` / `ResponseHead` への `#[non_exhaustive]` 付与を「将来のフィールド追加を非破壊的に扱えるようにする」と説明しているが、新規追加された `StatusClass` / `BodyKind` (バリアント追加) や全 Error enum には未付与で、設計方針として非対称になっている。

### 関連 issue

- 0017 (closed): Response フィールドを非公開化し `#[non_exhaustive]` を付与。本 issue は同じ方針を enum 側に適用する続編
- 0025 (closed): Request フィールドを非公開化し `#[non_exhaustive]` を付与
- 0031 (closed): RequestHead / ResponseHead フィールドを非公開化し `#[non_exhaustive]` を付与

## スコープ

### 付与対象 (30 個)

**全 Error enum (24 個)**: バリアント追加が頻発するため漏れなく付与する。

- `Error` (`src/error.rs:8`)
- `EncodeError` (`src/error.rs:59`)
- `AcceptError` (`src/accept.rs:25`)
- `AuthError` (`src/auth.rs:40`)
- `CacheError` (`src/cache.rs:32`)
- `ConditionalError` (`src/conditional.rs:34`)
- `ContentDispositionError` (`src/content_disposition.rs:30`)
- `ContentEncodingError` (`src/content_encoding.rs:23`)
- `ContentLanguageError` (`src/content_language.rs:22`)
- `ContentLocationError` (`src/content_location.rs:21`)
- `ContentTypeError` (`src/content_type.rs:32`)
- `CookieError` (`src/cookie.rs:33`)
- `DateError` (`src/date.rs:27`)
- `DigestFieldsError` (`src/digest_fields.rs:28`)
- `ETagError` (`src/etag.rs:29`)
- `ExpectError` (`src/expect.rs:36`)
- `HostError` (`src/host.rs:23`)
- `MultipartError` (`src/multipart.rs:37`)
- `RangeError` (`src/range.rs:33`)
- `TrailerError` (`src/trailer.rs:25`)
- `UpgradeError` (`src/upgrade.rs:22`)
- `UriError` (`src/uri.rs:36`)
- `VaryError` (`src/vary.rs:22`)
- `CompressionError` (`src/compression.rs:11`)

**状態 / 拡張余地のあるドメイン enum (6 個)**:

- `StatusClass` (`src/status_code.rs:329`) — IANA レジストリ拡張対応
- `BodyKind` (`src/decoder/body.rs:24`) — 本リリースで Tunnel 追加実績あり
- `BodyProgress` (`src/decoder/body.rs:45`) — decoder API 拡張余地
- `CompressionStatus` (`src/compression.rs:49`) — Compressor / Decompressor 拡張余地
- `Authorization` (`src/auth.rs:610`) — 認証スキーム追加 (RFC 7235 拡張)
- `AuthChallenge` (`src/auth.rs:656`) — 同上

### 付与対象外 (`Other` variant で拡張対応済み or RFC で構造固定)

- `ContentCoding` (`src/content_encoding.rs:50`) — `Other(String)` で拡張対応済み
- `DispositionType` (`src/content_disposition.rs:85`) — `Unknown(String)` で拡張対応済み
- `RequestTargetForm` (`src/request_target.rs:7`) — RFC 9112 §3.2 で 4 種固定
- `DayOfWeek` (`src/date.rs:78`) — 7 日固定
- `SameSite` (`src/cookie.rs:149`) — RFC 6265bis で 3 値固定 + `#[default]`
- `ETagList` (`src/etag.rs:248`) — RFC 9110 §13.1.1/13.1.2 で `Any / Tags(Vec)` 固定
- `RangeSpec` (`src/range.rs:62`) — RFC 9110 §14.1.2 で 3 形式固定
- `IfRange` (`src/conditional.rs:218`) — RFC 9110 §13.1.5 で `ETag / Date` 固定

これらは RFC で構造的に固定されており、新規バリアントが追加される蓋然性が極めて低い。

## 対応方針

### コード変更

対象 30 個の各 enum 定義の直上に `#[non_exhaustive]` を付与する。

```rust
#[non_exhaustive]
#[derive(Debug, thiserror::Error, ...)]
pub enum EncodeError { ... }
```

### テスト

`#[non_exhaustive]` は **同じ crate 内** からは exhaustive に match 可能なので、本 crate 内の既存テストはそのまま動作する (要 cargo test で確認)。

新規テストは追加しない。CHANGES.md でユーザー周知する。

### CHANGES.md

`## develop` に `[CHANGE]` として追加する:

```
- [CHANGE] 公開 enum (全 Error enum 24 個 + StatusClass / BodyKind / BodyProgress / CompressionStatus / Authorization / AuthChallenge) に `#[non_exhaustive]` を付与する
  - 本リリースだけでも EncodeError / AuthError / ContentDispositionError に複数バリアントを追加しており、未付与のままだとバリアント追加ごとに後方互換のない変更を出し続ける構造になっていた
  - BodyKind は本リリースで `Tunnel` バリアントを追加、StatusClass は新規導入のため、IANA レジストリ / API 拡張に備える
  - 利用側は `match` 文末尾に `_ =>` wildcard arm を追加する必要がある
  - `ContentCoding::Other` / `DispositionType::Unknown` のように `Other` バリアントで拡張対応済みの enum、`RequestTargetForm` / `DayOfWeek` / `SameSite` / `ETagList` / `RangeSpec` / `IfRange` のように RFC で構造的に固定された enum は付与対象外
  - @voluntas
```

### ブランチ

`feature/change-public-enums-non-exhaustive` (`feature/change-` prefix、後方互換なし、issue 番号を含まない)。

## 受け入れ基準

- 対象 30 個の公開 enum すべてに `#[non_exhaustive]` が付与されている
- `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace --exclude http11_client --exclude http11_server` がすべて PASS
- CHANGES.md `## develop` に `[CHANGE]` エントリが追加されている
- 付与対象外 8 個の enum には `#[non_exhaustive]` が付与されていない (意図を保持)

## RFC 参照

- 本 issue は Rust API ガイドライン由来であり、特定の RFC 仕様には依存しない
