# URI パーサーが相対参照の先頭 segment のコロンを scheme と誤認

- Priority: High
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-uri-parser-relative-colon-scheme
- Polished: {YYYY-MM-DD}

## 目的

`Uri::parse("abc:def")` のような相対参照を `scheme="abc"` / `path="def"` と誤認識する問題を修正し、RFC 3986 Section 4.2 / Section 3.3 に準拠する。

## 優先度根拠

High とする。RFC 3986 では relative-path reference の先頭 segment にコロンを含めることはできない。現状の誤認識は `scheme()` / `host()` / `path()` の値を変え、URI の正規化・解決・再エンコードの整合性を損なう。

## 現状

`src/uri.rs:563-578` および `src/uri.rs:358-472` のパーサーは、`:` より前が scheme 文字なら scheme として認識してしまう。`build_uri` での `./` 補完だけでは `Uri` オブジェクトの内部表現の誤認が残る。

## 設計方針

1. `//` なしで先頭 segment にコロンがある入力は relative reference として解析する。
2. `scheme=None`、`path="abc:def"` とするか、あるいは明確にエラーにする。
3. `normalize` の冪等性と整合性を保つ。

## 完了条件

- `Uri::parse("abc:def")` で `scheme()` が `None` になる（またはエラーになる）。
- 既存の絶対 URI パース（`http://...` 等）は影響を受けない。
- `pbt/tests/prop_uri.rs` / `tests/test_uri.rs` に該当ケースが追加される。

## 解決方法

- `src/uri.rs` のパースロジックで、authority なしかつ `//` なしの場合、先頭 segment のコロンを scheme 区切りとして扱わない。
- 必要に応じて `build_uri` の正規化経路も調整する。
- テストを追加する。
