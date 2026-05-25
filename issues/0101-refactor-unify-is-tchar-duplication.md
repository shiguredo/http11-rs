# is_tchar / is_unreserved / is_sub_delim 等の重複定義を一元化する

- Priority: Low
- Created: 2026-05-25
- Model: Opus 4.7
- Branch: feature/refactor-unify-is-tchar

## 目的

issue 0064 で `is_valid_token` / `is_token_char` の 12 重複定義を `validate.rs` に一元化したが、`const fn` 制約により `header_name.rs` と `method.rs` にモジュールローカルの `is_tchar` が残っている。また `host.rs` に `is_unreserved` / `is_sub_delim` / `is_hexdig` が `validate.rs` と重複して定義されている。

## 優先度根拠

コード重複の除去。フィールド追加や ABNF 変更時に全箇所への反映漏れリスクがある。ただし現時点で実害はなく、issue 0064 の残件として位置づける。

## 現状

### is_tchar の 3 重定義

| ファイル | 関数名 | 修飾 | 行 |
|---------|--------|------|-----|
| `src/validate.rs` | `is_token_char` | `pub(crate) fn` (非 const) | 11 |
| `src/header_name.rs` | `is_tchar` | `const fn` (モジュールローカル) | 95 |
| `src/method.rs` | `is_tchar` | `const fn` (モジュールローカル) | 93 |

3 関数とも同一の `matches!` 式 (RFC 9110 Section 5.6.2 tchar)。`header_name.rs` と `method.rs` は `const fn from_static` 内で使用するため `const fn` が必要。

### host.rs の重複

| ファイル | 関数名 | validate.rs 対応 | 行 |
|---------|--------|-----------------|-----|
| `src/host.rs` | `is_unreserved` | `is_unreserved_byte` | 245 |
| `src/host.rs` | `is_sub_delim` | `is_sub_delim_byte` | 249 |
| `src/host.rs` | `is_hexdig` | (なし、`u8::is_ascii_hexdigit`) | 256 |

### validate.rs 内の冗長関数

| 関数名 | 同一実装の関数 | 行 |
|--------|-------------|-----|
| `is_valid_method` | `is_valid_token` | 73 |

## 設計方針

### is_tchar の統一

`validate.rs` の `is_token_char` を `pub(crate) const fn` に変更し、`header_name.rs` と `method.rs` のモジュールローカル `is_tchar` を `use crate::validate::is_token_char` に置換する。

`const fn` 内で `matches!` マクロは使用可能 (Rust 1.46 以降) であり、MSRV 1.88 では問題ない。

### host.rs の統一

- `is_unreserved` → `use crate::validate::is_unreserved_byte` に置換
- `is_sub_delim` → `use crate::validate::is_sub_delim_byte` に置換
- `is_hexdig` → 呼び出し箇所で直接 `b.is_ascii_hexdigit()` に置換して関数を削除

### validate.rs の冗長関数

- `is_valid_method` を削除し、使用箇所を `is_valid_token` に置換する
- RFC 9110 Section 9.1 は `method = token` と定義しており、`is_valid_method` と `is_valid_token` が同一であることは RFC 上も正当
- `is_valid_method` の使用箇所: `src/encoder.rs` (method バリデーション)、`src/decoder/head.rs` (リクエスト行パース)。doc comment 内の参照名も `is_valid_token` に更新すること

### 命名方針

- `validate.rs` の関数名は `is_token_char` を維持する (RFC ABNF は `tchar` だが、`is_token_char` は `is_valid_token` と対で命名されており、既に 12 モジュールが import している既存名)
- `header_name.rs` / `method.rs` 内のローカル `is_tchar` が `use crate::validate::is_token_char` に置き換わるだけであり、外部 API には影響しない

## テスト戦略

- `is_token_char` を `const fn` に変更した後、既存の `compile_fail` doctest (`header_name.rs`, `method.rs`) が引き続きコンパイルエラーを報告することを確認する
- `header_name.rs` / `method.rs` の `from_static` は `const fn` 内で `is_token_char` を呼ぶため、`is_token_char` が `const fn` であることが必須。コンパイルが通ることで保証される
- 既存の PBT (`pbt/tests/prop_header_name.rs`, `pbt/tests/prop_method.rs`) がラウンドトリップを検証しているため、文字判定ロジックの等価性は PBT で担保される
- 新規テスト追加は不要 (全て既存テストでカバー済み)

## 完了条件

- `is_tchar` / `is_token_char` が `validate.rs` の 1 箇所のみに定義されていること
- `host.rs` のローカル文字種判定関数が `validate.rs` の関数を使用していること
- `is_valid_method` が削除され、使用箇所が `is_valid_token` に置換されていること
- 既存テスト (PBT / 単体テスト / fuzz) が全て通ること
