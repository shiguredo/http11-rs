# BasicAuth::parse の user-id 検証を BasicAuth::new に委譲する

- Priority: Medium
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-basicauth-colon-in-userid
- Polished: 2026-06-16

## 目的

`BasicAuth::parse` で Base64 デコード後の user-pass を分割した後の検証 (CTL チェック + 直接構築) を `BasicAuth::new` に委譲し、user-id コロン拒否 (`ColonInUserId`) と CTL 拒否 (`ControlCharacter`) の policy を 1 箇所に集約する。これにより RFC 7617 Section 2 の制限 ("user-id containing a colon character is invalid" / "MUST NOT contain any control characters") を parse 側でも防御的に適用し、将来の改変による policy 乖離を防ぐ。

## 優先度根拠

Medium とする。現状の `find(':')` 分割では分割後の user-id スライスにコロンが入ることはなく、`has_control_chars` 個別検証も `BasicAuth::new` 内の同等チェックを再現している。**機能的に既知のバグは存在しない**。本 issue の主目的は「validation policy の集約 (リファクタリング)」と「parse 経路にも防御的に同じチェックを通すことで将来の改変リスクを下げる」点。コード品質改善であり緊急性は High ではない。

なおファイル名 prefix が `bug-` となっているが、内容は実質的に refactor (機能挙動は維持・policy 集約) である。本 issue の magnet PR では `feature/fix-` ブランチを維持するが、関連 issue 起票時はカテゴリを `refactor` 側で起票すべき (リネームは将来 `create-issue` 経由で扱う候補)。

## 現状

`src/auth.rs:195-197` で Base64 デコード後の文字列を `find(':')` で最初のコロンで分割している。分割後、`src/auth.rs:200-202` で `has_control_chars` を個別実行し、`BasicAuth { ... }` を直接構築している。`BasicAuth::new` (`src/auth.rs:147-158`) は同じく user-id のコロン拒否 (`ColonInUserId`) と CTL 拒否 (`ControlCharacter`) を行っており、parse 側もこのポリシーに統一すべき。

RFC 7617 Section 2 ではパスワード内のコロンは許容されるため、`user:pass:word` のような入力を誤って reject してはならない (現状実装はこのケースを正しく受理しており、本 issue でも維持する)。

## 設計方針

1. 分割後の `username` / `password` で `BasicAuth::new(username, password)?` を呼び出し、`ColonInUserId` / `ControlCharacter` の検証を `new` に委譲する。
2. `BasicAuth::new` は既にコロンを拒否しているため、`username.contains(':')` 相当の防御的チェックも `new` 経由で適用する。
3. パスワード部分にコロンが含まれるのは RFC 7617 Section 2 で許容されるため、引き続き受理する。

## 完了条件

- `BasicAuth::parse("Basic dXNlcjpwYXNzd29yZA==")` (`user:password`) は成功し、`username() == "user"` / `password() == "password"` であること (基本ケース、回帰防止)。
- `BasicAuth::parse("Basic dXNlcjpwYXNzOndvcmQ=")` (`user:pass:word`) は成功し、パスワード内のコロンが保持されること (`username() == "user"` / `password() == "pass:word"`)。RFC 7617 Section 2 でパスワード内コロンは許容される。
- `BasicAuth::new("user:pass", "hope")` は `AuthError::ColonInUserId` を返すこと (parse 経由と同じ policy)。
- `tests/test_auth.rs` の BasicAuth パーステスト (`test_basic_auth_parse_*` 近傍、例: 374 行目以降) に、パスワード内コロン受理ケースと user-id 内コロン拒否ケースを追加すること。
- `pbt/tests/prop_auth.rs` の BasicAuth プロパティテスト (88 行目以降) に、以下 2 系統のプロパティを追加すること。
  - `BasicAuth::new("user:has:colon", "pass")` のように user-id にコロンを含む credentials が `ColonInUserId` を返すこと (new 単体検証)
  - 上記と同等の user-id を Base64 エンコードした入力 (例: `dXNlcjpoYXM6Y29sb246cGFzcw==` = `user:has:colon:pass`) を `BasicAuth::parse` に渡すと、最初の `:` で分割される現状仕様により実際は `username() == "user"` / `password() == "has:colon:pass"` として成功する (これは parse の現状動作であり、policy 委譲後も挙動は変わらないことを担保する)
- `CHANGES.md` の `## develop` セクションの `### misc` 配下に `[UPDATE] BasicAuth::parse の user-id / CTL 検証を BasicAuth::new に委譲し、RFC 7617 Section 2 の policy を 1 箇所に集約する` エントリを追加すること (`shiguredo-changelog` 規約に従う。本 issue は機能的に既知のバグはなく、リファクタリング相当であるため `### misc` 配下の `[UPDATE]` で表現する)。

## 解決方法

- `src/auth.rs:195-197` の分割後、`BasicAuth::new(username, password)?` を呼び出して `BasicAuth` を構築する。
- `src/auth.rs:200-202` の個別の `has_control_chars` チェックは `new` に委譲するため削除する。
- `AuthError::ColonInUserId` は `src/auth.rs:70` に既存のバリアントなので追加は不要。
- `tests/test_auth.rs` と `pbt/tests/prop_auth.rs` にテストを追加する。
