# BasicAuth::parse で user-id コロン検証を BasicAuth::new と共通化する

- Priority: High
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-basicauth-colon-in-userid
- Polished: 2026-06-13

## 目的

`BasicAuth::parse` で Base64 デコード後の user-pass を分割した後、`BasicAuth::new` と同様に user-id にコロンが含まれていないことを `ColonInUserId` で防御的に検証し、RFC 7617 Section 2 の制限を parse 側でも明示的に適用する。

## 優先度根拠

High とする。RFC 7617 Section 2 では「user-id containing a colon character is invalid」と明記されている。現状の `find(':')` による最初のコロン分割では user-id スライスにコロンが入ることはないが、検証を `BasicAuth::new` と共通化していないため、将来の改変時にポリシーがずれるリスクがある。

## 現状

`src/auth.rs:195-197` で Base64 デコード後の文字列を `find(':')` で最初のコロンで分割している。分割後、`src/auth.rs:200-202` で制御文字を個別に検証し、`BasicAuth { ... }` を直接構築している。`BasicAuth::new` (`src/auth.rs:147-158`) は同じく user-id のコロン拒否と CTL 拒否を行っており、parse 側もこのポリシーに統一すべき。

RFC 7617 Section 2 ではパスワード内のコロンは許容されるため、`user:pass:word` のような入力を誤って reject してはならない。

## 設計方針

1. 分割後の `username` / `password` で `BasicAuth::new(username, password)?` を呼び出し、`ColonInUserId` / `ControlCharacter` の検証を `new` に委譲する。
2. `BasicAuth::new` は既にコロンを拒否しているため、`username.contains(':')` 相当の防御的チェックも `new` 経由で適用する。
3. パスワード部分にコロンが含まれるのは RFC 7617 Section 2 で許容されるため、引き続き受理する。

## 完了条件

- `BasicAuth::parse("Basic dXNlcjpwYXNzd29yZA==")` (`user:password`) は成功し、`username() == "user"` / `password() == "password"` であること。
- `BasicAuth::parse("Basic dXNlcjpwYXNzOndvcmQ=")` (`user:pass:word`) は成功し、パスワード内のコロンが保持されること (`username() == "user"` / `password() == "pass:word"`)。
- `BasicAuth::parse("Basic dXNlcjpwYXNzOmhvcGU=")` (`user:pass:hope`) は成功し、`username() == "user"` / `password() == "pass:hope"` であること (コロンはパスワードに属する)。
- `BasicAuth::new("user:pass", "hope")` は `AuthError::ColonInUserId` を返すこと (parse 側の `new` 委譲により同じエラーが返るようになる)。
- `tests/test_auth.rs` の BasicAuth パーステスト (`test_basic_auth_parse_*` 近傍、例: 374 行目以降) に、パスワード内コロン受理ケースと user-id 内コロン拒否ケースを追加すること。
- `pbt/tests/prop_auth.rs` の BasicAuth プロパティテスト (88 行目以降) に、user-id にコロンを含む無効な credentials を生成し `BasicAuth::new` で `ColonInUserId` となるプロパティを追加すること。
- `CHANGES.md` の `## develop` セクションに `[FIX] BasicAuth::parse の user-id コロン / CTL 検証を BasicAuth::new と共通化し、RFC 7617 Section 2 の制限を防御的に適用する` エントリを追加すること。

## 解決方法

- `src/auth.rs:195-197` の分割後、`BasicAuth::new(username, password)?` を呼び出して `BasicAuth` を構築する。
- `src/auth.rs:200-202` の個別の `has_control_chars` チェックは `new` に委譲するため削除する。
- `AuthError::ColonInUserId` は `src/auth.rs:70` に既存のバリアントなので追加は不要。
- `tests/test_auth.rs` と `pbt/tests/prop_auth.rs` にテストを追加する。
