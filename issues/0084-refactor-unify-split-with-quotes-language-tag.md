# split_with_quotes と is_valid_language_tag の重複を validate.rs に統合する

- Priority: Low
- Created: 2026-05-15
- Model: deepseek v4-pro

## 目的

`split_with_quotes` が `src/accept.rs:577-603` と `src/expect.rs:206-231` で完全に同一の実装として重複している。`is_valid_language_tag` も `src/content_language.rs:89-116` と `src/accept.rs:619-641` で重複している。

## 現状

両関数とも 2 ファイルに同一ロジックがコピペされている。

## 設計方針

両関数を `src/validate.rs` に `pub(crate)` で移動し、元のファイルから import する。

## 完了条件

- `split_with_quotes` と `is_valid_language_tag` が `validate.rs` にのみ存在すること
- 全テストが通過すること
- `CHANGES.md` の `## develop` の `### misc` に `[UPDATE]` エントリが追加されていること
