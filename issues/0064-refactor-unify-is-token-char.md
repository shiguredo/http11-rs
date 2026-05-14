# 0064 refactor is_token_char / is_valid_token の重複定義を validate.rs に一元化する

Created: 2026-05-14
Model: deepseek-v4-pro

## 概要

`validate.rs` が `pub(crate) fn is_token_char(b: u8) -> bool` と `pub(crate) fn is_valid_token(s: &str) -> bool` を提供しているにもかかわらず、以下の 12 モジュールで完全に同一の実装が重複定義されている:

- `src/accept.rs:642-652` (`is_valid_token` + `is_token_char`)
- `src/auth.rs:932-942` (`is_valid_token` + `is_token_char`)
- `src/content_disposition.rs:400-413` (`is_valid_token` + `is_token_char`)
- `src/content_encoding.rs:162-172` (`is_valid_token` + `is_token_char`)
- `src/content_type.rs:314-327` (`is_valid_token` + `is_token_char`)
- `src/cookie.rs:507, 528-533` (`is_valid_cookie_name` 内で利用、`is_token_char` 定義)
- `src/digest_fields.rs:354-364` (`is_valid_token` + `is_token_char`)
- `src/expect.rs:233-243` (`is_valid_token` + `is_token_char`)
- `src/range.rs:489-504` (`is_valid_token` + `is_token_char`)
- `src/trailer.rs:101-111` (`is_valid_token` + `is_token_char`)
- `src/upgrade.rs:151-161` (`is_valid_token` + `is_token_char`)
- `src/vary.rs:106-116` (`is_valid_token` + `is_token_char`)

全 12 モジュールの実装は validate.rs の定義と `matches!` ブロックがバイト単位で一致することを確認済み (RFC 9110 Section 5.6.2 tchar = `"!" / "#" / "$" / "%" / "&" / "'" / "*" / "+" / "-" / "." / DIGIT / ALPHA / "^" / "_" / "`" / "|" / "~"`)。

RFC 9110 Section 5.6.2 の token ABNF が将来変更された場合、12 箇所すべてを個別に修正する必要があり、修正漏れのリスクが高い。

### 置換対象外

- `fuzz/fuzz_targets/fuzz_decoder_roundtrip.rs:32-36` の `is_valid_token` は非 tchar 文字 `;=` を含む独自定義であり、置換してはいけない。fuzz クレートはライブラリクレートから import できないため独自定義が必要。

### スコープ外（本 issue では扱わない）

- `needs_quoting` の 4 重複 (auth/accept/expect/content_type) — 実装に差異あり (expect は空文字判定あり)。統合には設計判断が必要なため別 issue で扱う。
- `escape_quotes` の 4 重複 — 0063 で対応済み (validate.rs に集約)。
- `is_valid_cookie_name` (`cookie.rs:506-508`) の `validate::is_valid_token` 置換 — cookie-name = token で置換可能だが、cookie モジュールの責務独立性を尊重し本 issue では触れない。

## 推奨対応

各モジュールから重複定義 `is_token_char` / `is_valid_token` を削除し、`use crate::validate::{is_token_char, is_valid_token};` に置換する。

cookie.rs は `is_token_char` のみを置換する。`is_valid_cookie_name` の呼び出しは維持。

モジュール内の `#[cfg(test)] mod tests` で `use super::*;` している場合、`is_token_char` / `is_valid_token` の削除でコンパイルエラーになる可能性がある。各モジュールのテストコード内でこれらの関数を直接参照している場合は `use crate::validate::...` を追加する。

## 確認手順

```bash
cargo check --workspace
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

## CHANGES.md

`## develop` の `### misc` に以下を追記する:

```
- [UPDATE] `is_token_char` / `is_valid_token` の 12 重複定義を `validate.rs` に一元化する
  - accept / auth / content_disposition / content_encoding / content_type / cookie / digest_fields / expect / range / trailer / upgrade / vary の重複を削除し `validate::{is_token_char, is_valid_token}` に置換する
  - @voluntas
```

## ブランチ名

`feature/add-unify-is-token-char`
(後方互換ありの内部リファクタリング → `feature/add-` prefix)

## 受け入れ基準

- [ ] 12 モジュールの重複 `is_token_char` / `is_valid_token` 定義が削除されている
- [ ] 各モジュールに `use crate::validate::{is_token_char, is_valid_token};` が追加されている
- [ ] `cargo check --workspace` が pass
- [ ] `cargo clippy --workspace -- -D warnings` が pass (未使用 import 警告なし)
- [ ] `cargo test --workspace` が pass (全テスト pass)
- [ ] `CHANGES.md` にエントリが追記されている
