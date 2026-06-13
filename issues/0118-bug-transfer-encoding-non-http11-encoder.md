# Transfer-Encoding が HTTP/1.1 以外でもエンコーダーで許可される

- Priority: High
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-transfer-encoding-non-http11-encoder
- Polished: 2026-06-13

## 目的

エンコーダー側で `version != "HTTP/1.1"` のメッセージに対して Transfer-Encoding ヘッダーが付いている場合をエラーにし、デコーダー側の厳格化と対称にする。

## 優先度根拠

High とする。RFC 9112 Section 6.1 により Transfer-Encoding ヘッダーは HTTP/1.1 で追加されたものであり、HTTP/1.1 以外のメッセージではフレーミングが faulty と見なされる。デコーダーは既に非 HTTP/1.1 の Transfer-Encoding を拒否しているが (`src/decoder/request.rs:305-315`、`src/decoder/response.rs:386-400`)、エンコーダーは許可しており、非対称な HTTP Request Smuggling / Response Smuggling の足場となる。

RFC 9112 Section 6.1:

> A client MUST NOT send a request containing Transfer-Encoding unless it knows the server will handle HTTP/1.1 requests (or later minor revisions)
> A server MUST NOT send a response containing Transfer-Encoding unless the corresponding request indicates HTTP/1.1 (or later minor revisions)

## 現状

- `src/decoder/request.rs:305-315` : `determine_body_kind` 内で `version != "HTTP/1.1"` かつ Transfer-Encoding: chunked のリクエストを拒否。
- `src/decoder/response.rs:386-400` : `determine_body_kind` 内で `version != "HTTP/1.1"` かつ Transfer-Encoding ヘッダーが存在するレスポンスを拒否。
- `src/encoder.rs:614-687` (`encode_request`): Transfer-Encoding と Content-Length の競合 (`src/encoder.rs:628-630`) のみチェックし、HTTP バージョンに関わらず Transfer-Encoding を許可。
- `src/encoder.rs:700-812` (`encode_response`): Transfer-Encoding と Content-Length の競合、1xx / 204 / 205 への Transfer-Encoding 禁止をチェックするが、HTTP バージョンによる TE 禁止はない。
- `src/encoder.rs:915-975` (`encode_request_headers`): `encode_request` と同様に TE/CL 競合のみチェック。
- `src/encoder.rs:991-1068` (`encode_response_headers`): `encode_response` と同様に TE/CL 競合と 1xx / 204 / 205 への TE 禁止のみチェック。

## 設計方針

1. `encode_request` / `encode_response` / `encode_request_headers` / `encode_response_headers` のいずれでも、`version != "HTTP/1.1"` かつ Transfer-Encoding ヘッダーが存在する場合はエラーにする。
   - バージョン比較は case-sensitive とし、デコーダー側 (`src/decoder/request.rs:310`、`src/decoder/response.rs:391`) と一致させる。`Request::with_version` / `Response::with_version` は大文字小文字を許容するが、Transfer-Encoding を送信する権利は `"HTTP/1.1"` との exact match のみに与える。
2. 新規エラーバリアント `TransferEncodingNotAllowed { version: String }` を `src/error.rs` の `EncodeError` に追加する。
   - 既存の `ForbiddenTransferEncoding { status_code: u16 }` は 1xx / 204 / 205 レスポンス専用であり、同名・別フィールドのバリアントを追加することはできないため、名称を区別する。
   - リクエスト・レスポンス両方で `TransferEncodingNotAllowed { version }` を使い、違反したバージョン文字列を保持する。
3. RTSP/1.0 / RTSP/2.0 (RFC 2326 Section 5 / RFC 7826) でも Transfer-Encoding は定義されていないため、拒否対象とする。
4. Transfer-Encoding の値 (`chunked` / `gzip` 等) を問わず、ヘッダーが存在するだけで拒否する。RFC 9112 Section 6.1 は Transfer-Encoding ヘッダー自体を HTTP/1.1 の機能と規定しているため。

## 完了条件

- 以下の全ての関数で、非 HTTP/1.1 バージョン (`"HTTP/1.0"` / `"RTSP/1.0"` / `"FOO/1.0"` / `"HTTP/2.0"` 等) に Transfer-Encoding ヘッダーが付いたリクエスト・レスポンスがエラーになること。
  - `encode_request`
  - `encode_response`
  - `encode_request_headers`
  - `encode_response_headers`
- `"HTTP/1.1"` での Transfer-Encoding は引き続き許可されること。
- 新規 `EncodeError::TransferEncodingNotAllowed { version }` バリアントを追加すること。
- `tests/test_encoder/main.rs` に以下のテストを追加すること。
  - `encode_request` / `encode_request_headers` で非 HTTP/1.1 + Transfer-Encoding を拒否するテスト
  - `encode_response` / `encode_response_headers` で非 HTTP/1.1 + Transfer-Encoding を拒否するテスト
  - `HTTP/1.1` + Transfer-Encoding が引き続き成功するリグレッションテスト
- `CHANGES.md` の develop セクションに `[FIX]` エントリを追加すること。

## 解決方法

- `src/encoder.rs` の `encode_request` / `encode_response` / `encode_request_headers` / `encode_response_headers` に、非 HTTP/1.1 での Transfer-Encoding 存在チェックを追加する。
- `src/error.rs` に `EncodeError::TransferEncodingNotAllowed { version: String }` を追加し、`fmt::Display` 実装も追加する。
- `tests/test_encoder/main.rs` にテストを追加する。
- `CHANGES.md` にエントリを追加する。

## 参考

- RFC 9112 Section 6.1 (Transfer-Encoding)
- RFC 9112 Section 6.2 (Content-Length と Transfer-Encoding の関係)
- RFC 2326 Section 5 / RFC 7826 (RTSP には Transfer-Encoding は定義されていない)
- `src/decoder/request.rs:305-315`
- `src/decoder/response.rs:386-400`
