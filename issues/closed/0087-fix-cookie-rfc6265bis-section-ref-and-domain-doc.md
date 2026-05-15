# Cookie の RFC 6265bis 節番号誤りを修正し、is_valid_domain_value の DNS ラベル制約を文書化する

- Priority: Low
- Branch: feature/fix-cookie-rfc6265bis-section-ref-and-domain-doc
- Created: 2026-05-15
- Model: deepseek v4-pro

## 目的

1. `src/cookie.rs:289` が RFC 6265bis Section 6.3 を参照しているが、この節番号は存在しない。正しくは Section 5.1.2 (Canonicalized Host Names)。
2. `is_valid_domain_value` が DNS ラベル制約 (先頭/末尾ハイフン禁止、空ラベル禁止、ラベル長 1-63、全体長 253) を検証していないことをコメントで明記する。

## 現状

`src/cookie.rs:286-291`:
```
/// RFC 6265bis Section 6.3 (IDNA Dependency):
```

`src/cookie.rs:523-526`:
```rust
fn is_valid_domain_value(value: &str) -> bool {
    // LDH (letter/digit/hyphen) + dot 判定のみ
```

## 設計方針

1. 節番号を `Section 5.1.2` に修正する
2. `is_valid_domain_value` の doc コメントに「RFC 1034 Section 3.5 のラベル制約 (leading/trailing hyphen 禁止、空ラベル、長さ上限) は現時点では検証していない」と追記する

## 完了条件

- 節番号が `Section 5.1.2` に修正されていること
- DNS ラベル制約の未検証がコメントで明示されていること
- CHANGES.md の ## develop に [FIX] エントリが追加されていること
