# Transfer-Encoding が HTTP/1.1 以外でもエンコーダーで許可される

- Priority: High
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-transfer-encoding-non-http11-encoder
- Polished: 2026-06-16

## 目的

エンコーダー側で `version != "HTTP/1.1"` のメッセージに対して Transfer-Encoding ヘッダーが付いている場合をエラーにし、デコーダー側の厳格化と対称にする。

### 0116 / 0116a との関係

0116 (HTTP-version 厳格化) 完了後は、`Request::with_version` / `Response::with_version` / `RequestHead::with_version` / `ResponseHead::with_version` は `is_valid_http_version` 通過のみ受理する。これにより `HTTP/1.1` 以外でも `HTTP/0.9` / `HTTP/1.0` / `HTTP/2.0` / `HTTP/3.0` 等の HTTP-version は通過するが、RTSP/1.0 / RTSP/2.0 は `with_version` 経由では到達不能になる。

本 issue は 0116 完了を前提に、**HTTP-version 内 (`HTTP/0.9` / `HTTP/1.0` / `HTTP/2.0` / `HTTP/3.0` 等) で `HTTP/1.1` 以外** のメッセージに Transfer-Encoding が付与されるのを encoder で防ぐ。RTSP 経路の Transfer-Encoding 拒否は 0116a (RTSP 用 API) 側の責務として、本 issue では扱わない。

このため、`with_version` 経由の API 入力では 0116 と本 issue の二重防御となるが、二重防御の意義は (a) `with_version` を経由しない `RequestHead::from_validated_parts` 等の内部経路への防御、(b) `encode_request` / `encode_response` を library 利用者が直接呼ぶ場合への明示的エラー報告、にある。

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
   - バージョン比較は case-sensitive とし、デコーダー側 (`src/decoder/request.rs:310`、`src/decoder/response.rs:391`) と一致させる。
   - 0116 完了後は `with_version` が `is_valid_http_version` で case-sensitive 検証するため二重防御となるが、`from_validated_parts` 等の内部経路と library 利用者の直接 encode 呼び出しへの防御として保持する。

2. 新規エラーバリアント `ForbiddenTransferEncodingForVersion { version: String }` を `src/error.rs` の `EncodeError` に追加する。
   - 既存の `ForbiddenTransferEncoding { status_code: u16 }` は 1xx / 204 / 205 レスポンス専用であり、フィールド型が異なるため名称を `ForbiddenTransferEncodingForVersion { version }` として明確に区別する (前周指摘の `TransferEncodingNotAllowed { version }` ではなく、既存命名規則と一貫させた `ForbiddenTransferEncodingForVersion` を採用)。
   - リクエスト・レスポンス両方で `ForbiddenTransferEncodingForVersion { version }` を使い、違反したバージョン文字列を保持する。
   - `fmt::Display` 実装: `"Transfer-Encoding not allowed for version {:?} (RFC 9112 Section 6.1)"`。

3. RTSP/1.0 / RTSP/2.0 の Transfer-Encoding 拒否は **本 issue では扱わない**。0116 (HTTP-version 厳格化) 完了により `with_version` 経由では RTSP は到達不能。RTSP 用 API は 0116a で別途整備され、そこで RTSP 自体の Transfer-Encoding 拒否を扱う (RFC 2326 §19.2 / RFC 7826 で Transfer-Encoding は HTTP から **明示的に除外** されているため、RTSP 用 API でも同等のチェックが必要)。

4. Transfer-Encoding の値 (`chunked` / `gzip` 等) を問わず、ヘッダーが存在するだけで拒否する。RFC 9112 Section 6.1 は Transfer-Encoding ヘッダー自体を HTTP/1.1 の機能と規定しているため。

## 完了条件

- 0116 (HTTP-version 厳格化) が完了マージ済みであること (本 issue の前提)。
- 0116a (RTSP 用 API) issue の起票時に、RTSP 用 API でも Transfer-Encoding を拒否する旨を 0116a 完了条件に **必ず含める** こと。引き継ぎ漏れ防止のため、`create-issue` 経由で 0116a を起票する際に「Transfer-Encoding 拒否を 0118 から引き継ぎ」を明記する。
- 以下の全ての関数で、非 HTTP/1.1 の HTTP-version (`"HTTP/0.9"` / `"HTTP/1.0"` / `"HTTP/2.0"` / `"HTTP/3.0"` 等) に Transfer-Encoding ヘッダーが付いたリクエスト・レスポンスがエラーになること。
  - `encode_request`
  - `encode_response`
  - `encode_request_headers`
  - `encode_response_headers`
- `"HTTP/1.1"` での Transfer-Encoding は引き続き許可されること。
- 新規 `EncodeError::ForbiddenTransferEncodingForVersion { version: String }` バリアントを追加すること (`fmt::Display` 実装も追加)。
- `tests/test_encoder/main.rs` に以下のテストを追加すること。
  - `encode_request` / `encode_request_headers` で非 HTTP/1.1 (`HTTP/1.0` / `HTTP/2.0` 等) + Transfer-Encoding を拒否するテスト
  - `encode_response` / `encode_response_headers` で非 HTTP/1.1 + Transfer-Encoding を拒否するテスト
  - `HTTP/1.1` + Transfer-Encoding が引き続き成功するリグレッションテスト
- `CHANGES.md` の `## develop` セクションに `[CHANGE]` エントリを追加すること (`EncodeError` への新規バリアント追加は破壊的変更のため `[FIX]` ではなく `[CHANGE]`)。`[ADD]` の下、`### misc` の上に配置する。例: `[CHANGE] encoder で非 HTTP/1.1 の HTTP-version に Transfer-Encoding が付いている場合をエラーにし、decoder 側の厳格化と対称化する。EncodeError に ForbiddenTransferEncodingForVersion { version } を追加する。`

## 解決方法

設計方針に沿って次の順序で実装する。

1. 0116 (HTTP-version 厳格化) のマージ後に着手する。
2. `src/error.rs` に `EncodeError::ForbiddenTransferEncodingForVersion { version: String }` バリアントを追加し、`fmt::Display` 実装も追加する。
3. `src/encoder.rs` の `encode_request` (L614) / `encode_response` (L700) / `encode_request_headers` (L915) / `encode_response_headers` (L991) に、非 HTTP/1.1 での Transfer-Encoding 存在チェックを追加する。チェック位置は既存の TE/CL 競合検査 (`L628` / `L705` / `L930` / `L997` 等) の直後または直前。
4. `tests/test_encoder/main.rs` に完了条件のテストを追加する。
5. `CHANGES.md` に `[CHANGE]` エントリを追加する。

## 参考: RFC 文面

- RFC 9112 Section 6.1 Transfer-Encoding
  > A client MUST NOT send a request containing Transfer-Encoding unless it knows the server will handle HTTP/1.1 requests (or later minor revisions); such knowledge might be in the form of specific user configuration or by remembering the version of a prior received response.
  > A server MUST NOT send a response containing Transfer-Encoding unless the corresponding request indicates HTTP/1.1 (or later minor revisions).
- RFC 9112 Section 6.2 Content-Length
  > A sender MUST NOT send a Content-Length header field in any message that contains a Transfer-Encoding header field.
- RTSP の Transfer-Encoding 扱い (要約、原文は `refs/rtsp/` 参照)
  - RFC 2326 (RTSP/1.0) §5: general-header の一覧に Transfer-Encoding は含まれない (HTTP/1.1 から継承する general-header の対象から除外)
  - RFC 7826 (RTSP/2.0) §6 前文 / §5.4 周辺: `RTSP does not support the HTTP/1.1 "chunked" transfer coding` および Transfer-Encoding ヘッダの非サポート
  - 上記より、本 issue 完了後に 0116a で導入される RTSP 用 API でも同等の Transfer-Encoding 拒否チェックが必要

## 関連ソース参照

- `src/decoder/request.rs:305-315` (decoder の HTTP-version × Transfer-Encoding 検証)
- `src/decoder/response.rs:386-400` (同上)
- refs/rtsp/rfc2326.txt §19.2 (RTSP/1.0 の Transfer-Encoding 除外)
- refs/rtsp/rfc7826.txt (RTSP/2.0)
