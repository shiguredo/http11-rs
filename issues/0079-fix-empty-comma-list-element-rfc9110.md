# parse_content_length_value と digest_fields の空カンマ区切り要素を RFC 9110 Section 5.6.1.2 に従いスキップする

- Priority: Medium
- Created: 2026-05-15
- Model: deepseek-v4-pro

## 目的

RFC 9110 Section 5.6.1.2: "A recipient MUST parse a received field-value as a list of values by applying OWS stripping and then splitting on commas. An empty list element MUST be ignored." に反し、`parse_content_length_value` (`src/decoder/body.rs:1399`) と `parse_dictionary` (`src/digest_fields.rs:297`) が空要素をエラーとしている。

## 優先度根拠

- RFC MUST 要件違反だが、悪用可能性は低い (Content-Length の文脈では空要素を許容しても安全側)
- 他のモジュール (Transfer-Encoding parser 等) では空要素を正しく無視している

## 現状

`src/decoder/body.rs:1399-1404`:
```rust
if part.is_empty() {
    return Err(Error::InvalidData(
        "invalid Content-Length: empty value in list".to_string(),
    ));
}
```

`src/digest_fields.rs:297-298`:
```rust
if part.is_empty() {
    return Err(DigestFieldsError::InvalidFormat);
}
```

## 設計方針

両方の箇所で `continue` に変更し空要素をスキップする。空スキップ後に値が 0 個になった場合はエラーとする。

## 完了条件

- `Content-Length: 100, ` (末尾カンマ) が受理され 100 と解釈されること
- `Content-Length: , ` (空要素のみ) はエラーになること
- `Digest` 系ヘッダーでも同様の挙動になること
