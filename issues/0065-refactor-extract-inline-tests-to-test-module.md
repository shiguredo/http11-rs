# 0065 refactor 内部テストを tests/test_<module>.rs に外部化する

Created: 2026-05-14
Model: deepseek-v4-pro

## 概要

以下の公開モジュールの単体テストが `src/<module>.rs` 内の `#[cfg(test)] mod tests` ブロックに存在するが、CLAUDE.md:93「単体テストのファイル名は `tests/test_<module>.rs` とし、`src/<module>.rs` に対応させること」に違反している:

- `src/compression.rs` → `tests/test_compression.rs` 不在
- `src/content_language.rs` → `tests/test_content_language.rs` 不在
- `src/etag.rs` → `tests/test_etag.rs` 不在
- `src/trailer.rs` → `tests/test_trailer.rs` 不在
- `src/upgrade.rs` → `tests/test_upgrade.rs` 不在
- `src/vary.rs` → `tests/test_vary.rs` 不在

いずれも公開 API をテストする内容であり、外部テストファイルに配置すべき。

## 対象ファイルとテスト数

- `src/content_language.rs:118-170` → 6 テスト
- `src/etag.rs:291-448` → 21 テスト
- `src/trailer.rs:174-389` → 17 テスト
- `src/upgrade.rs:163-206` → 5 テスト
- `src/vary.rs:118-167` → 5 テスト
- `src/compression.rs:297-545` → 約 20 テスト

## 推奨対応

各モジュールの `#[cfg(test)] mod tests` ブロックから公開 API テストを `tests/test_<module>.rs` に移動し、プライベート関数のテストのみを `src/<module>.rs` 内に残す (存在する場合)。

移動に伴い `use` 文や `mod` 宣言を適切に調整する。
