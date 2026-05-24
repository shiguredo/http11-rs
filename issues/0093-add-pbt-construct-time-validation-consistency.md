# 構築時検査の PBT 整合性検証を整備する

- Priority: Medium
- Created: 2026-05-23
- Model: Opus 4.7

## 目的

issue 0091 で導入する `HeaderName` / `Method` / `Scheme` の構築時検査について、
以下の不変性を PBT (proptest) で恒常的に検証する。

1. **完全性**: `new()` が `Ok` を返す入力集合が RFC の ABNF 定義（`token = 1*tchar` 等）と一致する
2. **非破壊性**: `new()` が受理した値の `as_bytes()` が入力バイト列と一致する（正規化による破壊がないこと）
3. **同値性**: case-insensitive 型（`HeaderName` / `Scheme`）の `Eq` が期待通り case-insensitive に動作する

decoder との受理集合一致検証は fuzzing の責務（任意バイト列を含む HTTP メッセージの構築 + decoder feed が必要なため）。`from_static` との一貫性は trybuild（0092）と `src/` 内の単体テストで担保する（`from_static` は `&'static [u8]` を要求し、`Box::leak` による PBT はメモリリークを起こし「お手本」に反するため）。

## 優先度根拠

Medium。0091 と同じ。構築型の内部実装（token 検査ロジック、`Eq` / `Hash` の手動実装）は PBT で恒常的に検証しないとサイレントに崩れるリスクがある。0091 完了直後に着手すべき。

## 現状

http11 の既存 PBT（`pbt/tests/`）はメッセージ全体のラウンドトリップ（`prop_request.rs` / `prop_response.rs` 等）はあるが、個別フィールド型の構築点と文字種検証の整合性を検証する仕組みはまだない。

## 設計方針

### テストの責務分担

| 検証内容 | 手法 | 理由 |
|---|---|---|
| `new()` の受理集合が ABNF と一致 | PBT | 任意の有効/無効バイト列を生成可能 |
| `new()` が非破壊的（`as_bytes()` 一致） | PBT | 同上 |
| case-insensitive `Eq` の動作 | PBT + 単体テスト | 境界ケースは単体テストで補完 |
| `from_validated_parts` の動作 | 単体テスト (`src/` 内) | `pub(crate)` のため外部クレートからアクセス不可 |
| `from_static` と `new` の一貫性 | 単体テスト (`src/` 内) | `from_static` が `&'static [u8]` を要求するため |
| decoder との受理集合一致 | fuzzing | 任意バイト列を含む HTTP メッセージの構築が必要 |

### 戦略 (Strategy) 集約

`pbt/src/lib.rs` に各構築時検査型の `valid_*` 戦略を追加する（既存戦略群と同じ場所にフラットに配置する。新規 `strategies` モジュールは作らず、既存の命名規則に従う）。

```rust
// pbt/src/lib.rs

/// RFC 9110 Section 5.6.2: token = 1*tchar
/// tchar = "!" / "#" / "$" / "%" / "&" / "'" / "*"
///       / "+" / "-" / "." / "^" / "_" / "`" / "|" / "~"
///       / DIGIT / ALPHA
/// 大文字 A-Z と小文字 a-z の両方を含む。
pub fn valid_header_name() -> impl Strategy<Value = Vec<u8>> {
    // 1 バイト以上の tchar を生成
    // ALPHA は b'A'..=b'Z' と b'a'..=b'z' の両方を含む
}

/// 不正なヘッダー名: 空 / CR / LF / NUL / 区切り文字 ( ) , / : ; < = > ? @ [ \ ] { } / DQUOTE のいずれかを含む
pub fn invalid_header_name() -> impl Strategy<Value = Vec<u8>> {
    // 正当な token 文字 + 少なくとも 1 つの不正文字を含む
}

/// RFC 9110 Section 9.1: method = token
/// 小文字 a-z を含む（RFC 上正当）バイト列も生成する。
pub fn valid_method() -> impl Strategy<Value = Vec<u8>> {
    // 1 バイト以上の tchar を生成（case-sensitive なため全 ALPHA が対象）
}

pub fn invalid_method() -> impl Strategy<Value = Vec<u8>> {
    // 空 / CR / LF / NUL / 区切り文字のいずれか
}

/// RFC 3986 Section 3.1: scheme = ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )
/// token より制限が厳しい（`!`, `#`, `$` 等を含まない）。
/// 大文字 A-Z を含む（RFC 3986 "should accept uppercase"）。
pub fn valid_scheme() -> impl Strategy<Value = Vec<u8>> {
    // 先頭は ALPHA（大文字小文字両方）、2 文字目以降は ALPHA / DIGIT / "+" / "-" / "."
}

pub fn invalid_scheme() -> impl Strategy<Value = Vec<u8>> {
    // 空 / 数字開始 / token の範囲外文字 / コロン
}
```

### プロパティ実装

```rust
// pbt/tests/prop_header_name.rs (新規)
use proptest::prelude::*;
use shiguredo_http11::HeaderName;
use pbt::{valid_header_name, invalid_header_name};

proptest! {
    /// valid なバイト列は HeaderName::new が Ok を返す
    #[test]
    fn new_accepts_valid_names(name in valid_header_name()) {
        prop_assert!(HeaderName::new(&name).is_ok());
    }

    /// invalid なバイト列は HeaderName::new が Err を返す
    #[test]
    fn new_rejects_invalid_names(name in invalid_header_name()) {
        prop_assert!(HeaderName::new(&name).is_err());
    }

    /// 受理された値の as_bytes() は入力バイト列と一致する（非破壊性）
    #[test]
    fn as_bytes_returns_original(name in valid_header_name()) {
        let h = HeaderName::new(&name).unwrap();
        prop_assert_eq!(h.as_bytes(), name.as_slice());
    }

    /// case-insensitive な Eq: 大文字小文字の違いを無視する
    #[test]
    fn eq_is_case_insensitive(name in valid_header_name()) {
        let lower: Vec<u8> = name.iter().map(|b| b.to_ascii_lowercase()).collect();
        let upper: Vec<u8> = name.iter().map(|b| b.to_ascii_uppercase()).collect();
        let h1 = HeaderName::new(&lower).unwrap();
        let h2 = HeaderName::new(&upper).unwrap();
        prop_assert_eq!(h1, h2);
    }
}

// pbt/tests/prop_method.rs (新規)
//   - new_accepts_valid_methods
//   - new_rejects_invalid_methods
//   - as_bytes_returns_original
//   - eq_is_case_sensitive: 大文字小文字を区別する
//     (b"GET" と b"get" は異なるメソッドとして扱われる)

// pbt/tests/prop_scheme.rs (新規)
//   - HeaderName と同様のプロパティ群（case-insensitive）
//   - 先頭数字の reject 検証
```

### 単体テスト（`src/` 内）

`from_static` と `new` の一貫性、`from_validated_parts` の検証は `src/` 内の `#[cfg(test)] mod tests` で実装する。

```rust
// src/header_name.rs 内 #[cfg(test)] mod tests
#[test]
fn from_static_matches_new_known_inputs() {
    // from_static は &'static [u8] を要求するため、既知の文字列リテラルのみで検証
    for name in &[b"host", b"content-type", b"x-custom-header"] {
        assert_eq!(
            HeaderName::new(*name).unwrap(),
            HeaderName::from_static(name)
        );
    }
}

#[test]
fn from_validated_parts_matches_new() {
    // from_validated_parts 経由で構築した値が new 経由と等価であることを確認
    for name in &[b"host", b"Content-Type", b"X-Custom"] {
        let v1 = HeaderName::new(*name).unwrap();
        let v2 = HeaderName::from_validated_parts(
            std::borrow::Cow::Owned(name.to_vec())
        );
        assert_eq!(v1, v2);
    }
}
```

### 既存 PBT の追従

既存の `prop_request.rs` / `prop_response.rs` は 0091 の API 変更（`name: HeaderName`）に伴う破壊的変更への追従が必要。これは 0091 の責務であるが、0093 で新設する strategy 群が既存 PBT と共存できることを確認する。

## 完了条件

- `pbt/src/lib.rs` に `valid_header_name` / `invalid_header_name` / `valid_method` / `invalid_method` / `valid_scheme` / `invalid_scheme` 戦略が定義されている（既存のフラットな構造に追加）
- 各戦略が以下の文字種を含む:
  - `valid_header_name`: 大文字 A-Z を含む全 tchar
  - `valid_method`: 小文字 a-z を含む全 tchar
  - `valid_scheme`: 大文字 A-Z を含む `ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )`
- `pbt/tests/prop_header_name.rs` が新設され、以下のプロパティが実装されている:
  - `new_accepts_valid_names`
  - `new_rejects_invalid_names`
  - `as_bytes_returns_original`
  - `eq_is_case_insensitive`
- `pbt/tests/prop_method.rs` が新設され、同様のプロパティ（+ `eq_is_case_sensitive`）が実装されている
- `pbt/tests/prop_scheme.rs` が新設され、同様のプロパティ（+ `scheme_digit_start_rejected`）が実装されている
- `src/header_name.rs` 内の `#[cfg(test)] mod tests` に `from_static_matches_new_known_inputs` と `from_validated_parts_matches_new` が追加されている（`Method` / `Scheme` も同様）
- 既存の全 PBT・fuzz が通る
- `CHANGES.md` の `## develop` の `### misc` に `[ADD]` エントリが追加されている

## 解決方法

実装順:

1. `pbt/src/lib.rs` に 6 つの戦略を追加
2. `src/header_name.rs` 内の単体テストを追加
3. `src/method.rs` 内の単体テストを追加
4. `src/uri.rs` 内の Scheme 単体テストを追加
5. `pbt/tests/prop_header_name.rs` を新設
6. `pbt/tests/prop_method.rs` を新設
7. `pbt/tests/prop_scheme.rs` を新設
8. `cargo test -p pbt` で全 PBT が通ることを確認
9. `CHANGES.md` に追記

## 関連

- 0091（HeaderName / Method / Scheme 導入）に依存
- decoder との受理集合一致検証は fuzzing で別途対応する
