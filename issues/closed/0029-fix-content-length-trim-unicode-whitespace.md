# 0029: Content-Length パースで Unicode 空白を OWS 扱いしないようにする

Created: 2026-05-11
Completed: 2026-05-11
Model: Opus 4.7

## 概要

`src/encoder.rs::validate_content_length_headers` と `src/decoder/body.rs::parse_content_length_value` が `str::trim()` を使ってヘッダー値の前後空白を除去している。Rust の `str::trim()` は `char::is_whitespace`、すなわち SP / HTAB だけでなく U+00A0 (NBSP) / U+2000-200A / U+3000 等の Unicode 空白全般を除去する。

一方 `is_valid_field_value` はヘッダー値の文字集合検査で obs-text (0x80-0xFF) を許容するため、NBSP の UTF-8 表現 `0xC2 0xA0` のような multi-byte シーケンスがヘッダー値に通る。結果として `Content-Length: \u{A0}5` のような値が encoder では 5 として送信され、decoder では 5 として受理される。

OWS の定義に従い SP / HTAB のみを除去する `trim_ows` は既に `src/decoder/body.rs` 内に private 実装が存在するが、`validate_content_length_headers` (encoder) と `parse_content_length_value` (decoder) の双方からは参照されておらず、`str::trim()` 直呼びになっている。

## 根拠

### RFC

- RFC 9110 Section 5.6.3 ABNF: `OWS = *( SP / HTAB )`
- RFC 9110 Section 5.5 で field-value に obs-text (0x80-0xFF) を許容
- RFC 9110 Section 8.6 Content-Length は `1*DIGIT`
- RFC 9112 Section 11.2 HTTP Request Smuggling (CWE-444) の根本原因は「同じヘッダーを複数の parser が異なる解釈をすること」

### 攻撃シナリオ

1. 攻撃者が `Content-Length: \u{A0}5\r\n` を含むリクエストを送る
2. 本クレートは NBSP を `str::trim()` で除去し `5` として受理
3. 直列に並ぶ別実装の proxy が strict OWS 解釈で `\u{A0}5` を「数値ではない」と扱い、`Content-Length: 0` 相当として後段に流す
4. 両者で `Content-Length` 解釈が分裂 → request smuggling

### コードベース内の不整合

- `src/decoder/body.rs:668` には RFC 準拠の `trim_ows` が既に存在し、コメントで「Rust の `str::trim()` は Unicode 空白全般を除去するため使用しない」と明記
- ところが同ファイル `:1346` の `parse_content_length_value` では `part.trim()` を使っている
- `src/encoder.rs:600` の `validate_content_length_headers` も `value.trim()` を使っている

## 対応方針

### `src/validate.rs`

- `trim_ows(s: &str) -> &str` を `pub(crate)` で追加する
- 既存の `src/decoder/body.rs::trim_ows` (private) を本関数に置き換える

### `src/encoder.rs`

- `validate_content_length_headers` の `value.trim()` を `trim_ows(value)` に変更する

### `src/decoder/body.rs`

- `parse_content_length_value` の `part.trim()` を `trim_ows(part)` に変更する
- private な `trim_ows` を削除して `validate::trim_ows` を import する

### テスト

- `tests/test_encoder.rs`: `Content-Length: \u{A0}5` を含む `Response` の `encode()` が `EncodeError::InvalidContentLengthValue` を返すことを確認
- `tests/test_decoder.rs`: `Content-Length: \u{A0}5` を含むレスポンス受信が `Error::InvalidData` で reject されることを確認
- 既存の PBT `prop_encoder` / `prop_decoder` で SP/HTAB は引き続き trim されることを既存ケースで担保

### CHANGES.md

`## develop` のメイン (`### misc` ではなく主要変更) に `[FIX]` として追記する。HTTP Request Smuggling 経路を塞ぐ修正のため misc には置かない。

## 解決方法

- `src/validate.rs` に `pub(crate) fn trim_ows(s: &str) -> &str` を追加した。RFC 9110 Section 5.6.3 の OWS = *( SP / HTAB ) に準拠し、`char::is_whitespace` ではなくバイト単位で SP / HTAB のみを除去する
- `src/decoder/body.rs` の private な `trim_ows` 実装を削除し、`validate::trim_ows` を import して同名関数の重複を解消した
- `src/decoder/body.rs::parse_content_length_value` の `part.trim()` を `trim_ows(part)` に置き換えた
- `src/encoder.rs::validate_content_length_headers` の `value.trim()` を `trim_ows(value)` に置き換え、コメントで Unicode 空白を除去しない理由を明記した
- テスト追加:
  - `tests/test_encoder.rs::test_encode_response_content_length_with_nbsp_is_rejected`: NBSP (U+00A0) を含む Content-Length が `EncodeError::InvalidContentLengthValue` で拒否されることを確認
  - `tests/test_encoder.rs::test_encode_request_content_length_with_ideographic_space_is_rejected`: 全角空白 (U+3000) を含む Content-Length が拒否されることを確認
  - `tests/test_encoder.rs::test_encode_response_content_length_with_sp_htab_is_accepted`: SP / HTAB のみで構成される従来の OWS は引き続き trim されて受理されることを担保
  - `tests/test_decoder.rs::test_response_content_length_with_nbsp_is_rejected`: NBSP を含むレスポンス受信が `decode_headers` で拒否されることを確認
  - `tests/test_decoder.rs::test_request_content_length_with_ideographic_space_is_rejected`: 全角空白を含むリクエスト受信が拒否されることを確認
- `CHANGES.md` の `## develop` に `[FIX]` エントリを追加した
