# 0064 refactor is_token_char の重複定義を validate.rs に一元化する

Created: 2026-05-14
Model: deepseek-v4-pro

## 概要

`validate.rs` が `pub(crate) fn is_token_char` と `pub(crate) fn is_valid_token` を提供しているにもかかわらず、以下の 12 モジュールで同一実装が重複定義されている:

- `src/auth.rs:936-942`
- `src/content_disposition.rs:408-413`
- `src/content_type.rs:322-327`
- `src/content_encoding.rs:166-172`
- `src/accept.rs:646-652`
- `src/cookie.rs:505-510`
- `src/digest_fields.rs:358-364`
- `src/expect.rs:237-243`
- `src/range.rs:498-504`
- `src/trailer.rs:105-111`
- `src/upgrade.rs:155-161`
- `src/vary.rs:110-116`

RFC 9110 Section 5.6.2 の token ABNF が将来変更された場合、12 箇所すべてを個別に修正する必要があり、修正漏れのリスクが高い。

## 推奨対応

各モジュールの重複定義を削除し、`use crate::validate::is_token_char;` (または `is_valid_token`) に置換する。
