# 0062 fix head.rs と encoder.rs の str::trim() を trim_ows に置換する

Created: 2026-05-14
Model: deepseek-v4-pro

## 概要

`src/decoder/head.rs` の `is_keep_alive` / `is_chunked` と `src/encoder.rs` の 205 Content-Length 検証が `str::trim()` を使用している。`str::trim()` は `char::is_whitespace` に基づき NBSP (U+00A0) や U+2000-200A 等の Unicode 空白も除去するが、RFC 9110 Section 5.6.3 の OWS は `*( SP / HTAB )` のみと定義されている。

`is_valid_field_value` は obs-text (0x80-0xFF) を許容するため、`Connection: \u{00A0}keep-alive` や `Transfer-Encoding: \u{00A0}chunked` のような値が decoder を通過しうる。この値を `str::trim()` で評価する実装と `trim_ows` を使う前段代理で解釈不一致が生じ、HTTP Request Smuggling (CWE-444) の足場となる。

issue 0029 (Content-Length の trim_ows 化) および 0053 (Transfer-Encoding / Trailer の trim_ows 化) と同じ対策を未対応箇所に適用する。

## 再現手順

1. `Connection: \u{00A0}keep-alive` を含むリクエストを decoder が受理 (is_valid_field_value 通過)
2. `HttpHead::is_keep_alive` が `str::trim()` で NBSP を除去し `keep-alive` と解釈 → `true` を返す
3. 前段の reverse proxy が `trim_ows` で SP/HTAB のみ除去し `\u{00A0}keep-alive` は `keep-alive` と一致しないと判定 → 接続を close
4. 解釈不一致により Smuggling が成立する

## 対象ファイル

- `src/decoder/head.rs:91` (`is_keep_alive`: `token.trim()`)
- `src/decoder/head.rs:143` (`is_chunked`: `token.trim()`)
- `src/encoder.rs:771` (`encode_response`: `cl.trim()`)
- `src/encoder.rs:1041` (`encode_response_headers`: `cl.trim()`)

## 推奨対応

各箇所の `trim()` を `trim_ows()` (validate.rs で定義済み) に置換する。
