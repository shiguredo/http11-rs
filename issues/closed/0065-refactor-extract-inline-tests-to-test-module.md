# 0065 refactor 内部テストを tests/test_<module>.rs に外部化する

Created: 2026-05-14
Completed: 2026-05-14
Model: deepseek-v4-pro
Branch: feature/fix-extract-inline-tests

## 概要

以下の公開モジュールの単体テストが `src/<module>.rs` 内の `#[cfg(test)] mod tests` ブロックに存在するが、CLAUDE.md:93「単体テストのファイル名は `tests/test_<module>.rs` とし、`src/<module>.rs` に対応させること」に違反している:

| モジュール | テスト場所 | テスト内容 |
|---|---|---|
| `src/compression.rs` | `#[cfg(test)] mod tests` (line 297-545) | 14 テスト、うち 5 テストは `#[non_exhaustive]` enum の variant を直接構築 |
| `src/content_language.rs` | `#[cfg(test)] mod tests` (line 118-170) | 6 テスト、すべて公開 API 経由 |
| `src/etag.rs` | `#[cfg(test)] mod tests` (line 291-448) | 17 テスト、すべて公開 API 経由 |
| `src/trailer.rs` | `#[cfg(test)] mod tests` (line 174-389) | 20 テスト、すべて公開 API 経由 |
| `src/upgrade.rs` | `#[cfg(test)] mod tests` (line 163-206) | 5 テスト、すべて公開 API 経由 |
| `src/vary.rs` | `#[cfg(test)] mod tests` (line 118-167) | 5 テスト、すべて公開 API 経由 |

### compression.rs の特記事項

`CompressionStatus` と `CompressionError` は `#[non_exhaustive]` が付与された enum であり、外部クレート (`tests/` 以下) から構造体リテラルで variant を構築するとコンパイルエラーになる。以下の 5 テストが該当する:

- `CompressionStatus::Continue { consumed: n, produced: m }` 等の直接構築
- `CompressionError::BufferTooSmall { required: n, available: m }` 等の直接構築

これらのテストは NoCompression の `compress()` の戻り値から variant を取得する形に書き換える。例:

```rust
// Before (src/compression.rs 内):
let status = CompressionStatus::Continue { consumed: 10, produced: 8 };

// After (tests/test_compression.rs 内):
use shiguredo_http11::compression::{NoCompression, CompressionStatus};
let mut c = NoCompression::new();
let status = c.compress(b"hello", &mut [0u8; 8]).unwrap();
```

## 推奨対応

各モジュールの `#[cfg(test)] mod tests` ブロックから公開 API テストを `tests/test_<module>.rs` に移動する。

### 書き換えパターン

内部テスト (`src/<module>.rs`):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    // ...
}
```

外部テスト (`tests/test_<module>.rs`):
```rust
use shiguredo_http11::<module>::*;
// または use shiguredo_http11::<module>::Foo;
```

`use super::*;` を削除し、必要な型・関数を crate path で import する。

### Cargo.toml

`tests/*.rs` は Cargo により自動的にテストバイナリとして認識されるため、`[[test]]` セクションの追加は不要。

## スコープ外

- `tests/test_decode_body.rs` — 命名規則違反 (対応する `src/decode_body.rs` が存在しない)。0066 で `tests/test_decoder/` に統合予定のため本 issue では触れない。

## 確認手順

```bash
cargo test -p shiguredo_http11 --test test_compression
cargo test -p shiguredo_http11 --test test_content_language
cargo test -p shiguredo_http11 --test test_etag
cargo test -p shiguredo_http11 --test test_trailer
cargo test -p shiguredo_http11 --test test_upgrade
cargo test -p shiguredo_http11 --test test_vary
cargo test --workspace  # 全テスト pass 確認
```

## CHANGES.md

`## develop` の `### misc` に以下を追記する:

```
- [UPDATE] src/<module>.rs 内のインラインテストを tests/test_<module>.rs に外部化する (CLAUDE.md:93 準拠)
  - 対象: compression / content_language / etag / trailer / upgrade / vary
  - @voluntas
```

## ブランチ名

`feature/fix-extract-inline-tests`
(CLAUDE.md 規約違反の修正 → `feature/fix-` prefix)

## 受け入れ基準

- [x] 6 モジュールの `#[cfg(test)] mod tests` から公開 API テストが `tests/test_<module>.rs` に移動されている
- [x] 各テストファイルで `use shiguredo_http11::<module>::...` の import が正しく設定されている
- [x] `cargo test --workspace` が pass
- [x] 移動前と同一のテスト数が pass している (67 / 67)
- [x] `CHANGES.md` にエントリが追記されている

## 解決方法

- 6 モジュール (compression / content_language / etag / trailer / upgrade / vary) から `#[cfg(test)] mod tests` ブロックを完全に削除した (合計 -776 行)
- 同名の `tests/test_<module>.rs` を新規作成し、`use super::*;` を `use shiguredo_http11::<module>::...` に書き換えた
- compression.rs の 5 テストは `CompressionStatus` の variant を直接構築していた経路を、`NoCompression::compress` / `finish` 経由でヘルパー関数 (`make_continue` / `make_output_full` / `make_complete`) を使って取得する形に書き換えた
- issue 本文は `BufferTooSmall { required, available }` を `#[non_exhaustive]` enum の struct variant のため外部からは構築不可と記載していたが、実装上は構築可能と判明したため、`test_compression_error_display` で 5 variant 全てを検証する完全網羅形にした
- 全 6 ファイルのファイルヘッダー doc コメントを既存 tests/ 配下の多数派表記である「ユニットテスト」に揃えた
- `CHANGES.md` の `## develop` の `### misc` に `[UPDATE]` エントリを追記した
