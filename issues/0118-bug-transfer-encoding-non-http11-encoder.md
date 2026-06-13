# Transfer-Encoding が HTTP/1.1 以外でもエンコーダーで許可される

- Priority: High
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-transfer-encoding-non-http11-encoder
- Polished: {YYYY-MM-DD}

## 目的

エンコーダー側で `version != "HTTP/1.1"` のメッセージに対して Transfer-Encoding ヘッダーが付いている場合をエラーにし、デコーダー側の厳格化と対称にする。

## 優先度根拠

High とする。RFC 9112 Section 6.1 により chunk 転送コーディングは HTTP/1.1 のフレーミング機能である。デコーダーは既に非 HTTP/1.1 の Transfer-Encoding を拒否しているが、エンコーダーは許可しており、非対称な HTTP Request Smuggling の足場となる。

## 現状

- `src/decoder/request.rs:305-315` : HTTP/1.1 以外の TE を拒否。
- `src/decoder/response.rs:391-400` : 同上。
- `src/encoder.rs:614-630` / `700-733` : バージョンによらず TE を許可。

## 設計方針

1. `encode_request` / `encode_response` / `encode_response_headers` で `version != "HTTP/1.1"` かつ TE 存在の場合をエラーにする。
2. エラーバリアントは既存の `ConflictingTransferEncodingAndContentLength` または新規 `ForbiddenTransferEncoding { version }` を検討する。
3. RTSP 等の他プロトコルで TE が必要なケースがないか確認する。

## 完了条件

- `"HTTP/1.0"` / `"RTSP/1.0"` / `"FOO/1.0"` 等のバージョンで TE が付いたメッセージがエンコード時にエラーになること。
- `"HTTP/1.1"` での TE は引き続き許可されること。
- テストが追加されること。

## 解決方法

- `src/encoder.rs` の `validate_request_fields` / `validate_response_fields` に、非 HTTP/1.1 での TE 存在チェックを追加する。
- 適切な `EncodeError` バリアントを追加または再利用する。
- テストを追加する。
