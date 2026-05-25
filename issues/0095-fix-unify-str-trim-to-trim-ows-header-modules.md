# str::trim() を trim_ows() に統一しヘッダーパースモジュールの OWS 処理を RFC 準拠にする

- Priority: High
- Created: 2026-05-25
- Model: Opus 4.7
- Branch: feature/fix-unify-str-trim-to-trim-ows

## 目的

`str::trim()` は `char::is_whitespace` に基づき NBSP (`U+00A0`) / 全角スペース (`U+3000`) / `U+2000`-`U+200A` 等の Unicode 空白を SP/HTAB と同等に除去する。RFC 9110 Section 5.6.3 は OWS を `*( SP / HTAB )` と定義しており、これ以外の文字を OWS として除去することは RFC 違反である。

`is_valid_field_value` は obs-text (0x80-FF) を許容するため、Unicode 空白の UTF-8 表現がヘッダー値に到達可能である。RFC 9110 Section 5.5 は obs-text を opaque data として扱うべき (SHOULD) としており、`str::trim()` がこれらを除去することは opaque data の意図しない破壊にあたる。

また、フレーミングヘッダー (`Content-Length` / `Transfer-Encoding`) については前段プロキシとの OWS 解釈不一致が HTTP Request Smuggling (CWE-444) の足場となりうる。非フレーミングヘッダーでは直接の smuggling 経路にはならないが、RFC MUST 要件 (Section 5.5: field parsing implementation MUST exclude leading/trailing whitespace prior to evaluating the field value) への違反として統一的に修正する。

issue 0029 / 0053 / 0062 で `Content-Length` / `Transfer-Encoding` / `Connection` / `Trailer` のパースは `trim_ows()` に統一済みだが、それ以外のヘッダーパースモジュールが未対応のまま残っている。

## 優先度根拠

RFC 9110 Section 5.5 / Section 5.6.3 の MUST 要件違反。認証ヘッダー (`auth.rs`) や Cookie (`cookie.rs`) は OWS 解釈不一致による値の変質が認証バイパスやセッション固定に悪用される可能性がある。issue 0029/0053/0062 で同じ対策を適用した実績があり、残件として位置づけられる。

## RFC 根拠

- RFC 9110 Section 5.6.3: `OWS = *( SP / HTAB )` — OWS は SP (0x20) と HTAB (0x09) のみ
- RFC 9110 Section 5.5: 「A field value does not include leading or trailing whitespace. When a specific version of HTTP allows such whitespace to appear in a message, a field parsing implementation MUST exclude such whitespace prior to evaluating the field value.」
- RFC 9110 Section 5.5: `obs-text = %x80-FF` — 「A recipient SHOULD treat other allowed octets in field content (i.e., obs-text) as opaque data.」
- RFC 9112 Section 5: `field-line = field-name ":" OWS field-value OWS`

## 現状

以下のモジュールで `str::trim()` / `str::trim_start()` / `str::trim_end()` が OWS 除去の用途で使用されている (箇所数は `.trim()` 等の呼び出し数。1 行に複数呼び出しがある場合は個別にカウント):

| ファイル | 箇所数 | 置換種別 | 主な使用箇所 |
|---------|--------|---------|------------|
| `src/auth.rs` | 10 | trim_ows: 8, trim_ows_start: 2 | `BasicAuth::parse`, `DigestAuth::parse`, `BearerToken::parse`, `WwwAuthenticate::parse`, `strip_scheme` (行 762, 778 が trim_start) |
| `src/content_type.rs` | 12 | trim_ows: 12 | `ContentType::parse`, `split_media_type` (行 223 は 1 行に 2 呼び出し), `parse_parameters` (行 260, 283 は `trim_start_matches(';')` との複合) |
| `src/range.rs` | 13 | trim_ows: 13 | `Range::parse`, `parse_range_spec`, `ContentRange::parse`, `AcceptRanges::parse` |
| `src/content_disposition.rs` | 7 | trim_ows: 7 | `ContentDisposition::parse`, `parse_param_value`, `parse_ext_value` |
| `src/digest_fields.rs` | 6 | trim_ows: 6 | `parse_dictionary`, `parse_byte_sequence`, `parse_want_fields` |
| `src/expect.rs` | 5 | trim_ows: 5 | `Expect::parse`, `parse_token_or_quoted` |
| `src/cookie.rs` | 4 | trim_ows: 4 | `SetCookie::parse` (属性パース 行 255-256), `parse_cookie_pair` (行 483-484) |
| `src/cache.rs` | 5 | trim_ows: 5 | `CacheControl::parse` (行 114, 123, 129, 130), `Age::parse` (行 432) |
| `src/date.rs` | 4 | trim_ows: 2, trim_ows_start: 2 | `HttpDate::parse` (行 204, 212), `HttpDate::parse_rfc850` (行 240, 247) |
| `src/etag.rs` | 3 | trim_ows: 3 | `EntityTag::parse` (行 82), `parse_etag_list` (行 223, 234) |
| `src/content_encoding.rs` | 2 | trim_ows: 2 | `ContentEncoding::parse` (行 86, 91) |
| `src/content_language.rs` | 2 | trim_ows: 2 | `ContentLanguage::parse` (行 61, 65) |
| `src/content_location.rs` | 1 | trim_ows: 1 | `ContentLocation::parse` (行 54) |
| `src/multipart.rs` | 2 | trim_ows: 2 | multipart ヘッダーパース (行 437, 438) |
| `src/accept.rs` | 1 | trim_ows: 1 | `parse_media_range_item` (行 424) |
| `src/conditional.rs` | 1 | trim_ows: 1 | `IfRange::parse` (行 232) |

合計: 78 箇所 (trim_ows: 74, trim_ows_start: 4)

### 除外対象

- `src/decoder/body.rs:765`: `if name != name.trim()` — ヘッダー名の先行・後続空白の**検出** (バリデーション) であり OWS 除去ではない。Unicode 空白を含むヘッダー名も不正として reject すべきなので `str::trim()` の使用は妥当

## 設計方針

### 置換ルール

- 両端 `.trim()` → `trim_ows()`
- 先頭のみ `.trim_start()` → `trim_ows_start()`
- `trim_start_matches(';').trim()` → `trim_ows(rest.trim_start_matches(';'))` (セミコロン除去後に `trim_ows` で両端を除去)

### `trim_ows_start` の追加

`validate.rs` に `pub(crate) fn trim_ows_start(s: &str) -> &str` を追加する:

```rust
pub(crate) fn trim_ows_start(s: &str) -> &str {
    let bytes = s.as_bytes();
    let start = bytes
        .iter()
        .position(|&b| b != b' ' && b != b'\t')
        .unwrap_or(bytes.len());
    &s[start..]
}
```

使用箇所:
- `src/auth.rs:762` `input.trim_start()` → `trim_ows_start(input)`
- `src/auth.rs:778` `rest.trim_start()` → `trim_ows_start(rest)`
- `src/date.rs:212` `input[comma_pos + 1..].trim_start()` → `trim_ows_start(&input[comma_pos + 1..])`
- `src/date.rs:247` `input[comma_pos + 1..].trim_start()` → `trim_ows_start(&input[comma_pos + 1..])`

`trim_ows_end` は現時点で必要な箇所がないため追加しない。

#### date.rs の HTAB 許容についての注記

RFC 9110 Section 5.6.7 の IMF-fixdate / rfc850-date ABNF ではカンマ直後は `SP` (単一スペース) のみが規定されている。`trim_ows_start` は HTAB も許容するため RFC の ABNF よりも寛容になるが、以下の理由で `trim_ows_start` を使用する:
- 受信側の堅牢性 (Postel's Law): 実世界では HTAB を含む不正な日付ヘッダーが存在する
- 既存の `str::trim_start()` は既に HTAB 以外の Unicode 空白も許容しており、`trim_ows_start` は制限を強化する方向の変更である
- Date ヘッダーの OWS 厳格化 (SP のみ許容) は本 issue のスコープ外とし、必要であれば別 issue で対応する

### 後方互換

本変更は公開 API のシグネチャを変更しない。動作の変化:
- Unicode 空白 (NBSP 等) を先頭・末尾に含むヘッダー値が、従来は OWS として除去されて正常パースされていたが、修正後は OWS として除去されず field-value の一部として保持される
- その結果、パース結果が変わる可能性がある (例: `Content-Type: \u{00A0}text/html` は従来 `text/html` とパースされていたが、修正後は NBSP が残り `InvalidMediaType` エラーになる可能性がある)
- これは RFC 準拠動作への修正であり、バグ修正として `[FIX]` に分類する

## テスト戦略

### PBT (proptest)

既存の各モジュールの PBT Strategy に Unicode 空白文字を含む入力を生成する戦略を追加する必要はない。PBT は既に任意文字列ベースの Strategy で obs-text 含む入力を生成しており、`trim_ows` への変更後も既存テストが通ることで回帰がないことを確認できる。

### 単体テスト

以下のエッジケースを `tests/test_<module>.rs` に追加する:
- NBSP (`\u{00A0}`) を先頭・末尾に含むヘッダー値が OWS として除去されないこと
- SP / HTAB のみが正しく OWS として除去されること
- NBSP が field-value の一部として保持され、各パーサーの文字種検証で適切に reject または opaque 保持されること

対象モジュール (高リスク順): auth, cookie, content_type, range, content_disposition, date

date.rs は `trim_ows_start` の新規関数を使用する箇所であり、HTAB 許容の動作確認として含める。残りの 10 モジュールは既存 PBT の任意文字列 Strategy で間接的にカバーされるため、個別の単体テスト追加は不要。

### Fuzzing

既存の fuzz target が全モジュールのパニック安全性を担保している。`trim_ows` への変更は既存 fuzz target の入力空間をカバーしているため、新規 fuzz target は不要。

## 完了条件

- `src/` 内の全 `.rs` ファイルで `str::trim()` / `str::trim_start()` / `str::trim_end()` が OWS 除去の用途で使用されていないこと
- 以下のコマンドで OWS 除去用途の残存がないことを確認する:
  ```
  grep -rn '\.trim()\|\.trim_start()\|\.trim_end()' src/ | grep -v 'decoder/body.rs:765' | grep -v 'trim_ows'
  ```
  ここで検出される箇所は全て「OWS 除去用途ではない」ことを目視確認する。注意: `grep -v 'trim_ows'` は行全体を対象とするため、同一行に `trim_ows` と `.trim()` が共存するケースは検出できない。最終確認では `rg '\.trim\(\)|\.trim_start\(\)|\.trim_end\(\)' src/ --no-heading` の結果を全件目視すること
- `validate.rs` に `trim_ows_start` が追加されていること
- 既存テスト (PBT / 単体テスト / fuzz) が全て通ること
- Unicode 空白を含むヘッダー値が OWS として除去されず field-value の一部として保持されることを確認する単体テストが追加されていること (対象: auth, cookie, content_type, range, content_disposition, date の 6 モジュール)
