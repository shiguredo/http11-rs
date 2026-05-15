# detect_scheme と detect_request_target_form の encoder/decoder 間重複を統合する

- Priority: High
- Created: 2026-05-15
- Completed: 2026-05-15
- Model: deepseek v4-pro
- Branch: feature/refactor-unify-detect-scheme

## 解決方法

- `detect_scheme` 関数を `src/request_target.rs` に `pub(crate)` で移動し、encoder / decoder 両方から共有するようにした
- `src/encoder.rs` と `src/decoder/body.rs` の重複定義を削除した
- `detect_request_target_form` は encoder と decoder で検証ロジックとエラー型が異なるため、共通の検出部分のみ `detect_scheme` として抽出した

## 目的

`detect_scheme` 関数が `src/encoder.rs:285-306` と `src/decoder/body.rs:906-937` で一字一句同じ実装として重複している。`detect_request_target_form` も同様に encoder (`src/encoder.rs:224-241`) と decoder (`src/decoder/body.rs:792-828`) で準重複している。encoder/decoder 間の解釈不一致は HTTP Request Smuggling (CWE-444) の足場になり得る。

## 優先度根拠

- encoder/decoder 間の判定ロジック不一致は smuggling の根本原因の一つ
- 重複コードは修正の同期漏れリスクを生む
- RFC 9112 Section 3.2 の request-target 形式判定は encode/decode で共通であるべき

## 現状

**`detect_scheme`** (`src/encoder.rs:285-306` / `src/decoder/body.rs:906-937`):

22 行の関数が完全に同一。RFC 3986 Section 3.1 の scheme 構文検証。

**`detect_request_target_form`** (`src/encoder.rs:224-241` / `src/decoder/body.rs:792-828`):

判定順序 (`*` → `starts_with('/')` → `contains("://")` → authority-form → scheme) がほぼ同一だが、decoder 側は `validate_origin_form` / `validate_absolute_form` による追加検証を行い、encoder 側は `looks_like_authority_form` + `detect_scheme` にフォールバックする。

## 設計方針

1. `detect_scheme` を `request_target.rs` または `validate.rs` に `pub(crate)` で移動し、両者から共有する
2. `detect_request_target_form` の共通判定ロジックを `request_target.rs` に抽出し、追加検証は呼び出し側で行う形にする
3. encoder.rs の重複定義を削除する

## 完了条件

- `detect_scheme` が 1 箇所のみに存在すること
- `detect_request_target_form` の判定ロジックが 1 箇所に集約されていること
- 既存の `tests/test_request_target.rs` の全テストが引き続き通過すること
- `cargo test` で全テストが通過すること
- `CHANGES.md` の `## develop` の `### misc` に `[UPDATE]` エントリが追加されていること
