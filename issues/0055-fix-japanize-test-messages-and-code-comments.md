# 0055: 英語のテストメッセージとコードコメントを日本語化し AGENTS.md 規約に準拠する

Created: 2026-05-13
Model: Opus 4.7

## 概要

AGENTS.md (`CLAUDE.md` シンボリックリンク先) は以下を規約として明記している:

- コメントは全て日本語
- テストメッセージは全て日本語
- ログメッセージは全て英語
- エラーメッセージは全て英語

`/review-code` のレビューで以下の規約違反が検出された:

1. **テストメッセージの英語残存 (58 件以上)** — `prop_assert!` / `assert_eq!` の message が英語
2. **本体コードの英語コメント** — `src/encoder.rs` / `src/decoder/response.rs` / `src/decoder/request.rs` の `// Status line: ...` / `// Empty line - end of headers` 等
3. **examples の英語コメント** — `examples/http11_client/src/main.rs` の `// HTTPS` / `// HTTP`
4. **examples テストの英語 expect メッセージ** — `examples/http11_client/tests/*.rs` の `.expect("parse_url")` 等
5. **廃止 RFC への参照** — `pbt/tests/prop_*.rs` の `// HTTP トークン文字 (RFC 7230)` を RFC 9110 Section 5.6.2 に置き換える

ログメッセージ / エラーメッセージは引き続き英語のまま (規約遵守)。

## 根拠

### AGENTS.md 引用

```
- コメントは全て日本語
- ログメッセージは全て英語
- エラーメッセージは全て英語
- テストメッセージは全て日本語
```

### CLAUDE.md 引用

```
- RFC 7230 は廃止されて RFC 9110 になってる
- RFC 7231 は廃止されて RFC 9112 になってる
```

PBT 内に残っている `RFC 7230` 参照は token ABNF の出典として書かれているが、RFC 9110 Section 5.6.2 に移管されている。

## スコープ

### 対象

**テストメッセージ (主要箇所、全数は実装時に確定)**:

- `pbt/tests/prop_auth.rs`: L275, L290, L318, L341, L360, L380, L408, L425, L449, L470, L487, L505, L526, L549 等の `prop_assert!(false, "Expected ...")` / `unexpected error message: {}`
- `pbt/tests/prop_encoder.rs`: L462, L487, L525, L542, L671, L693, L715, L732 等
- `pbt/tests/prop_decoder/response.rs`: L329, L348, L370, L389, L456, L836, L858, L962 等
- `pbt/tests/prop_decoder/body.rs`: L572, L599 等
- `pbt/tests/prop_range.rs`: L153, L167, L231
- `pbt/tests/prop_request.rs`: L264 `"should decode headers"` (今回 0054 で追加した日本語メッセージは対象外)
- `tests/test_decoder.rs`: L124, L1749, L1771 等
- `tests/test_request.rs`: L56, L221, L229 等
- `examples/http11_client/tests/nginx_basic.rs` / `nginx_streaming.rs`: `.expect("parse_url")` 等

**本体コードの英語コメント**:

- `src/encoder.rs`: L137, L688, L716, L805, L838, L977, L993, L1072, L1088
- `src/decoder/response.rs`: L512, L574, L582
- `src/decoder/request.rs`: L406, L518, L526
- `examples/http11_client/src/main.rs`: L76, L80

**廃止 RFC 参照**:

- `pbt/tests/prop_request.rs:10` `// HTTP トークン文字 (RFC 7230)` → RFC 9110 Section 5.6.2
- `pbt/tests/prop_content_type.rs:10` `// 有効なトークン文字列 (RFC 7230)` → 同上
- `pbt/tests/prop_accept.rs:10` `// HTTP トークン文字 (RFC 7230) - 安全な文字のみ使用` → 同上

### 対象外

- ログメッセージ (`tracing::info!` / `warn!` / `error!` 等) は英語のまま (規約)
- エラーメッセージ (`Error::InvalidData("...")` / `thiserror` の `#[error("...")]`) は英語のまま (規約)
- `///` doc コメント本文は規約に明示がないため、本 issue では触らない (将来別 issue で検討)

## 対応方針

### テストメッセージの日本語化

`prop_assert!(false, "Expected Authorization::Basic")` のような panic メッセージを `"Authorization::Basic を期待"` 等に書き換える。

`.expect("parse_url")` 系も `expect("parse_url が失敗")` のように日本語に統一する。

### コメントの日本語化

`// Status line: ...` のような直訳的な英語コメントは:

- 自明な内容 (`// Headers` / `// Body` / `// End of headers`) は削除する
- 説明が必要な箇所は日本語に翻訳する

不要なコメントは削除する方針 (AGENTS.md「不要なコメントは禁止」)。

### 廃止 RFC 参照の更新

`(RFC 7230)` を `(RFC 9110 Section 5.6.2)` に置き換える。本体コードに残存していないか念のため grep で確認する。

### CHANGES.md

`## develop` の `### misc` サブセクションに以下を追加する:

```
### misc

- [FIX] テストメッセージとコードコメントを日本語化し AGENTS.md 規約に準拠させる
  - `pbt/tests/` および `tests/` の英語 `prop_assert!` / `assert!` メッセージを日本語に統一する
  - `src/encoder.rs` / `src/decoder/*.rs` / `examples/http11_client/src/main.rs` の英語コードコメントを日本語化または削除する
  - `pbt/tests/prop_request.rs` / `prop_content_type.rs` / `prop_accept.rs` の廃止 RFC 参照 (`RFC 7230`) を RFC 9110 Section 5.6.2 に更新する
  - 機能・ログメッセージ・エラーメッセージは変更しない
  - @voluntas
```

### ブランチ

`feature/fix-japanize-test-and-code-comments` (`feature/fix-` prefix、機能影響なし、issue 番号を含まない)。

## 受け入れ基準

- `pbt/tests/` および `tests/` 配下の `prop_assert!` / `assert!` / `assert_eq!` の message に英語が残っていない (機能名・型名・ヘッダー名等のコード由来識別子は除外)
- `src/encoder.rs` / `src/decoder/response.rs` / `src/decoder/request.rs` / `examples/http11_client/src/main.rs` の英語コードコメントが日本語化または削除されている
- `pbt/tests/prop_*.rs` の `RFC 7230` 参照が RFC 9110 Section 5.6.2 に置き換わっている
- `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace --all-targets` がすべて PASS
- CHANGES.md `## develop` の `### misc` に `[FIX]` エントリが追加されている
- ログメッセージ (`tracing::*`) およびエラーメッセージ (`Error::*` / `#[error(...)]`) は英語のまま (regression がない)

## RFC 参照

- RFC 9110 Section 5.6.2 (token ABNF、`refs/rfc9110.txt`)
