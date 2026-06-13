# RequestHead / ResponseHead の文字列系 API が String 非対応

- Priority: Medium
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/refactor-requesthead-responsehead-string-api
- Polished: {YYYY-MM-DD}

## 目的

`RequestHead` / `ResponseHead` の文字列を引数に取るコンストラクタやセッターが `&str` 固定になっているため、`String` や `Cow` を直接渡せるようにし、不要なクローンを削減する。

## 優先度根拠

Medium とする。`with_reason_phrase`, `with_status` 系等が `&str` 固定で、動的に構築した文字列を渡す際に `.to_string()` や `.clone()` が発生している。メッセージ生成のホットパスでは軽微ながらコストが積み上がる。

## 現状

- `src/request_head.rs` : `with_uri(&str)`, `with_version(&str)`, `with_method(&str)` 等。
- `src/response_head.rs` : `with_status(&str)`, `with_reason_phrase(&str)`, `with_version(&str)` 等。

## 設計方針

1. `impl Into<Cow<'_, str>>` を受け入れる形式に変更するか、`TryFrom<String>` 経路を追加する。
2. 既存の `&str` 呼び出しとの互換性を維持する。
3. バリデーションは `Cow` 借用時はゼロコピーで行えるよう維持する。

## 完了条件

- `String` / `&'static str` / `Cow` 等から `RequestHead` / `ResponseHead` を構築できること。
- 既存の `&str` 呼び出しがそのままコンパイルできること。
- テストが追加されること。

## 解決方法

- 各セッターの引数を `impl Into<Cow<'a, str>>` に変更する（ライフタイムを適切に管理）。
- または `TryFrom<String>` / `TryFrom<Cow>` 実装を追加する。
- 影響範囲を確認し、必要に応じて `examples/` を修正する。
- テストを追加する。
