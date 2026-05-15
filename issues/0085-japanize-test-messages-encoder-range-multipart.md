# テストメッセージを日本語に統一する (encoder.rs / range.rs / multipart.rs)

- Priority: Low
- Created: 2026-05-15
- Model: deepseek v4-pro

## 目的

AGENTS.md:9「コメントは全て日本語」に反し、以下のファイルで英語のテストメッセージ（`panic!` / `expect` 内の文字列）が使用されている。

## 現状

- `src/encoder.rs:1310, 1311, 1314-1316, 1321, 1322, 1325-1327`: `.expect("estimate overflow")` / `.expect("encode failed")` / `"estimate {} < output {}"`
- `src/range.rs:506, 521, 530`: `panic!("expected Range")` / `"expected Suffix"` / `"expected FromStart"`
- `src/multipart.rs:1036`: `panic!("unexpected error: {e:?}")`

## 設計方針

各メッセージを日本語に変更する。例: `"estimate overflow"` → `"容量見積もりがオーバーフロー"`、`"expected Range"` → `"Range バリアントを期待"`。

## 完了条件

- 上記全箇所のテストメッセージが日本語になっていること
- `cargo test` で全テストが通過すること
- `CHANGES.md` の `## develop` の `### misc` に `[UPDATE]` エントリが追加されていること
