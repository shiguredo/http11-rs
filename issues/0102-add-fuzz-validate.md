# validate.rs の fuzz ターゲットを追加する

- Priority: High
- Created: 2026-05-26
- Model: Opus 4.7
- Branch: feature/add-fuzz-validate

## 目的

`src/validate.rs` は crate 全体のバリデーション基盤であり、デコーダー・エンコーダー双方から呼ばれるが、直接の fuzz ターゲットが存在しない。状態遷移やインデックス走査を含むパーサーがあり、パニック安全性を直接検証する必要がある。

## 優先度根拠

- `parse_quoted_string` は Accept / Content-Type / Expect 等の全 quoted-string ヘッダーが依存する基盤関数
- `is_valid_request_target` のパーセントエンコーディング検証に off-by-one があると HTTP Request Smuggling に直結し得る
- `split_with_quotes` のクォート内 delimiter スキップにバグがあると Accept パースが壊れる
- いずれも `pub(crate)` のため既存の横断的 fuzz ターゲットからは間接的にしか到達できない

## 現状

- `validate.rs` にはインラインの `#[cfg(test)]` テストが `escape_quotes` のみ存在する
- `parse_quoted_string` / `is_valid_request_target` / `split_with_quotes` は他モジュールのテスト経由で間接的にカバーされているが、任意入力に対するパニック安全性の直接的な fuzzing はない

## 設計方針

- `fuzz/fuzz_targets/fuzz_validate.rs` を新規作成する
- `pub(crate)` 関数のため、crate 内部の公開 API 経由ではなく `shiguredo_http11` の内部テスト用エクスポート、または同 crate 内の公開関数経由で間接的にファズする
  - もし直接ファズできない場合は、対象関数を呼び出す公開 API (例: `Content-Type` パース等) に任意文字列を投入する形で代替する
- 対象関数:
  - `parse_quoted_string`: 任意 `&str` を投入し、パニックしないことを検証
  - `is_valid_request_target`: 任意 `&str` を投入し、パニックしないことを検証
  - `split_with_quotes`: 任意 `&str` × 任意 `char` (delimiter) を投入し、パニックしないことを検証
  - `escape_quotes`: 任意 `&str` を投入し、パニックしないことを検証

## 完了条件

- `fuzz/fuzz_targets/fuzz_validate.rs` が作成されている
- `fuzz/Cargo.toml` に `[[bin]]` エントリが追加されている
- `cargo fuzz build fuzz_validate` が成功する
- 上記 4 関数の経路が fuzz ターゲットでカバーされている
