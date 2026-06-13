# BasicAuth::parse が user-id にコロンを含む入力を誤って受理

- Priority: High
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-basicauth-colon-in-userid
- Polished: {YYYY-MM-DD}

## 目的

`BasicAuth::parse` で user-id 部分にコロンが含まれる無効な `user:pass:word` 形式の credentials を拒否し、RFC 7617 に準拠する。

## 優先度根拠

High とする。RFC 7617 Section 2 では「user-id containing a colon character is invalid」と明記されている。現状は最初のコロンで分割するだけで、user-id 内のコロンを検証しておらず、誤った user/password 分解釈を生む。

## 現状

`src/auth.rs:195-197` で Base64 デコード後の文字列を `find(':')` で最初のコロンで分割しているだけで、user-id 部分にコロンが含まれていないか確認していない。

## 設計方針

1. 分割後の user-id にコロンが含まれていれば専用のエラー `AuthError::ColonInUserId` を返す。
2. `BasicAuth::new` は既にコロンを拒否しているため、parse 側も同じポリシーに統一する。

## 完了条件

- `BasicAuth::parse("Basic dXNlcjpwYXNzOmhvcGU=")`（`user:pass:hope`）がエラーになること。
- `BasicAuth::parse("Basic dXNlcjpwYXNzd29yZA==")`（`user:password`）は成功すること。
- テストに該当ケースが追加されること。

## 解決方法

- `src/auth.rs:195-197` の分割後に `username.contains(':')` をチェックし、true なら `AuthError::ColonInUserId` を返す。
- `AuthError` に `ColonInUserId` バリアントを追加する（`#[non_exhaustive]` なので公開 API 影響あり）。
- `tests/test_auth.rs` と `pbt/tests/prop_auth.rs` にテストを追加する。
