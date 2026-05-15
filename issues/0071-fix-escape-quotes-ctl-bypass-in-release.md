# escape_quotes の CTL 検証を debug_assert! から常時有効なチェックに変更し、builder 系 API に入力検証を追加する

- Priority: High
- Created: 2026-05-15
- Model: deepseek-v4-pro

## 目的

`escape_quotes()` が CTL 文字 (CR/LF/NUL/0x01-08/0x0B-0C/0x0E-1F/0x7F) の検出に `debug_assert!` を使用しており、release ビルドでは完全にバイパスされる。builder 系インターフェース (`with_filename`, `with_parameter`, `WwwAuthenticate::basic` 等) は値検証を一切行っていないため、CTL 文字がそのまま出力され HTTP Response Splitting (CWE-113) の攻撃経路になる。

## 優先度根拠

- 全 builder API に波及する HTTP Response Splitting 経路
- `debug_assert!` は CI で release ビルド時は検出されない
- `escape_quotes` の呼び出し元 (auth / accept / content_type / expect / content_disposition) も値検証を escape 側に依存している

## 現状

`src/validate.rs:388-391`:
```rust
pub(crate) fn escape_quotes(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        debug_assert!(
            is_quoted_pair_char(c),
            "CTL char (CR/LF/NUL/...) must be rejected before reaching escape_quotes"
        );
```

コード内コメントでも「release ビルドでは通過する」と明記されている。RFC 9110 Section 5.5 は CTL を MUST reject。

## 設計方針

1. `debug_assert!` を `if !is_quoted_pair_char(c) { ... }` に変更し、release ビルドでも CTL を検出する
2. CTL 検出時は無害な文字 (例: `' '`) に置換するか、または `String` の代わりに `Result<String, _>` を返す
3. 長期的には全 builder API の入力値検証を追加するが、本 issue では escape 側の防御を最優先とする

## 完了条件

- `escape_quotes()` が release ビルドでも CTL 文字を検出し、安全に処理すること
- `cargo test` で全テストが通過すること
- `cargo clippy -- -D warnings` で警告がないこと
