# examples/http11_reverse_proxy の日本語エラーメッセージを英語に修正する

- Priority: Medium
- Created: 2026-05-15
- Model: deepseek-v4-pro

## 目的

`examples/http11_reverse_proxy/src/main.rs:722` で upstream 切断エラーが日本語文字列 `"接続が閉じられました"` になっている。AGENTS.md: `エラーメッセージは全て英語` に違反する。

## 現状

```rust
return Err("接続が閉じられました".into());
```

## 設計方針

英語メッセージ (`"upstream connection closed"`) に変更する。

## 完了条件

- エラーメッセージが英語になっていること
