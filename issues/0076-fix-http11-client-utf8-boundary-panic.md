# examples/http11_client のボディ出力で UTF-8 文字境界パニックを修正する

- Priority: High
- Created: 2026-05-15
- Model: deepseek-v4-pro

## 目的

`examples/http11_client/src/main.rs:104` で `&text[..1000]` によりボディテキストを truncated しているが、マルチバイト UTF-8 文字がバイトインデックス 1000 を跨ぐ場合に panic し、アプリ全体がクラッシュする。サンプルは「お手本」であるため許容できない。

## 優先度根拠

- お手本コードで panic 経路があることは CLAUDE.md:37「サンプルはお手本なので性能と堅牢性を両立させること」に違反
- HTTP レスポンスボディにマルチバイト文字を含むことは一般的であり、発生頻度が無視できない

## 現状

`examples/http11_client/src/main.rs:101-104`:
```rust
if let Ok(text) = std::str::from_utf8(body) {
    if text.len() > 1000 {
        info!(total_bytes = body.len(), "Body truncated");
        println!("{}...", &text[..1000]);  // ここで panic 可能性
```

Rust の `&str[..n]` はバイト境界チェックを runtime で行い、境界違反時は `thread panicked at byte index 1000 is not a char boundary` でアプリ全体がクラッシュする。

## 設計方針

`text.floor_char_boundary(1000)` (Rust 1.79+) または `text.char_indices().take_while(|(i, _)| *i < 1000).map(|(_, c)| c).collect::<String>()` で安全な truncate に変更する。

## 完了条件

- 1000 バイト目がマルチバイト文字の途中であるボディテキストを与えても panic しないこと
- truncate されたテキストが有効な UTF-8 であること
