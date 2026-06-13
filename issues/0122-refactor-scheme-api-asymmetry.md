# Scheme / SchemeError の API が Method / HeaderName と非対称

- Priority: Medium
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/refactor-scheme-api-asymmetry
- Polished: 2026-06-13

## 目的

`Scheme` / `SchemeError` の API を `Method` / `HeaderName` と対称にし、同じ使い勝手（ `'static` リテラルからの `TryInto`、エラーへの原因入力保持、`core::error::Error` 実装など）を提供する。 scheme の構文は RFC 3986 Section 3.1 に準拠する。

## 優先度根拠

Medium とする。`Method` / `HeaderName` には存在する `'static str` リテラルからの `TryFrom` や `input()` / `into_input()`、`core::error::Error` 実装が `Scheme` / `SchemeError` にはないため、ユーザーの認知負荷が高く、エラーメッセージの質も低下する。

## 現状

- `src/uri.rs:1070`: `Scheme` 定義。
- `src/uri.rs:1073-1083`: `SchemeError` は `Empty` / `InvalidFirstByte { byte }` / `InvalidByte { byte, position }` を持ち、`input` フィールドがない。
- `src/uri.rs:1085`: `SchemeError` は `core::error::Error` / `Clone` / `PartialEq` / `Eq` を実装していない。
- `src/uri.rs:1116`: `Scheme::new()` は `SchemeError` を返すが、原因入力を保持しない。
- `src/uri.rs:1173`: `Scheme::from_static()` は `const fn` として提供されている。
- `src/uri.rs:1191` / `src/uri.rs:1196`: `Scheme` は `as_bytes()` / `as_str()` を持つ。
- `src/uri.rs:1202-1216`: `Scheme` は case-insensitive な `PartialEq` / `Eq` / `Hash` を実装済み。
- `src/uri.rs:1230`: `Scheme` は `Display` を実装済み。
- `src/lib.rs:113`: `Scheme` / `SchemeError` を re-export。
- `src/method.rs:42` / `src/header_name.rs:44`: `MethodError` / `HeaderNameError` は `Empty { input }` / `InvalidByte { byte, position, input }` と `input()` / `into_input()`、`core::error::Error`、`Clone` / `PartialEq` / `Eq` を実装している。
- `src/method.rs:197` / `src/method.rs:220`: `Method` は `TryFrom<&'static str>` / `TryFrom<&'static [u8]>` を実装している。
- `src/method.rs:266` / `src/header_name.rs:286`: `Method` / `HeaderName` は `From<Method> for String` / `From<HeaderName> for String` を実装している。
- `src/header_name.rs:201` / `src/header_name.rs:224`: `HeaderName` も `TryFrom<&'static str>` / `TryFrom<&'static [u8]>` を実装している。

## 設計方針

1. `SchemeError` の名前は他の構築型と同じく `SchemeError` のままとする。`InvalidScheme` は `UriError::InvalidScheme` などと混同しやすく、他の `XxxError` 命名とも不一致である。
2. `SchemeError` のバリアントを `MethodError` / `HeaderNameError` と同じ形状に統一する。`InvalidFirstByte` は先頭バイト制約も `InvalidByte { byte, position: 0, input }` で表現できるため削除し、`Empty { input }` / `InvalidByte { byte, position, input }` とする。
3. `SchemeError` に `input()` / `into_input()` を追加し、`Clone` / `PartialEq` / `Eq` / `core::error::Error` を実装する。
4. `Scheme` に `TryFrom<&'static str>` / `TryFrom<&'static [u8]>` を実装する。成功時は `Cow::Borrowed` を利用する。
5. `Scheme` に `From<Scheme> for String` を実装する。
6. `Scheme::new()` のエラー生成時に入力文字列を保持する。
7. `Scheme` のドキュメントコメントに `Method` / `HeaderName` と同様の構築経路表と、非 `'static` な `&str` がコンパイルエラーになることの説明を追加する。

## 完了条件

- `src/uri.rs:1073-1083` の `SchemeError` が `Empty { input }` / `InvalidByte { byte, position, input }` のみを持ち、`input()` / `into_input()`、`Clone` / `PartialEq` / `Eq` / `core::error::Error` を実装すること。
- `src/uri.rs:1116` の `Scheme::new()` がエラー時に `input` を含む `SchemeError` を返すこと。
- `src/uri.rs` に `impl TryFrom<&'static str> for Scheme` / `impl TryFrom<&'static [u8]> for Scheme` が追加され、それぞれ `Scheme::new()` と同じ受理集合になること。
- `src/uri.rs` に `impl From<Scheme> for String` が追加されること。
- `src/lib.rs:113` の re-export は `Scheme` / `SchemeError` のまま維持されること。
- `tests/test_uri.rs` に `TryFrom<&'static str>` / `TryFrom<&'static [u8]>` / `input()` / `into_input()` / `core::error::Error` 実装のテストを追加すること。
- `pbt/tests/prop_scheme.rs` に `TryFrom` と `Scheme::new()` の受理集合一致を検証するプロパティテストを追加すること。
- `CHANGES.md` の `## develop` に以下を追加すること。
  - `[CHANGE] SchemeError のバリアントを MethodError / HeaderNameError と同じ形状に統一し、input フィールドと input() / into_input()、Clone / PartialEq / Eq / core::error::Error を追加する`
  - `[ADD] Scheme に TryFrom<&'static str> / TryFrom<&'static [u8]> / From<Scheme> for String を実装する`

## 解決方法

- `src/uri.rs` の `SchemeError` 定義と `impl` ブロックを `MethodError` / `HeaderNameError` と同じ形に書き換える。
- `src/uri.rs` に `TryFrom<&'static str>` / `TryFrom<&'static [u8]>` / `From<Scheme> for String` を追加する。
- `Scheme::new()` 内の `SchemeError::Empty` / `SchemeError::InvalidByte` 生成箇所で入力文字列を `String::from_utf8_lossy(...).into_owned()` で保持する。
- `tests/test_uri.rs` / `pbt/tests/prop_scheme.rs` に対称性を確認するテストを追加する。
- `CHANGES.md` に破壊的変更と追加を記載する。
