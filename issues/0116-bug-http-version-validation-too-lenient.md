# HTTP-version の構文が RFC 9112 より緩い

- Priority: High
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-http-version-validation-too-lenient
- Polished: 2026-06-13

## 目的

HTTP-version の検証を RFC 9112 Section 2.3 に厳密に準拠させ、`"HTTP" "/" DIGIT "." DIGIT` 以外の形式を HTTP-version として拒否する。RTSP 等の他プロトコルは維持するため、HTTP 専用検証を新設し既存の汎用検証は存続させる。

## 優先度根拠

High とする。RFC 9112 では `HTTP-version = HTTP-name "/" DIGIT "." DIGIT`、`HTTP-name = %x48.54.54.50 ; HTTP` と明記されている。現状の `is_valid_protocol_version` は `token "/" 1*DIGIT "." 1*DIGIT` を許容しており、`"http/1.1"`（小文字）、`"HTTP/11.1"`（major 多桁）、`"HTTP/1.11"`（minor 多桁）、`"FOO/1.0"` 等を HTTP バージョンとして受理してしまう。これにより送信側で RFC 違反のバージョン文字列を生成できる経路が残っている。

## 現状

- `src/validate.rs:78-118` の `is_valid_protocol_version` は `token "/" 1*DIGIT "." 1*DIGIT` を許容している。
- `src/encoder.rs:16-21` の `is_valid_version_for_encode` は VCHAR のみをチェックするため、HTTP-version として不適切な値も通過する。
- `Request::with_version` (`src/request.rs:112` 付近) / `Response::with_version` (`src/response.rs:144` 付近) / `RequestHead::with_version` (`src/decoder/head.rs:195` 付近) / `ResponseHead::with_version` (`src/decoder/head.rs:385` 付近) / `decoder/request.rs` (`src/decoder/request.rs:393` 付近) / `decoder/response.rs` (`src/decoder/response.rs:464` 付近) は上記 `is_valid_protocol_version` を使用しており、HTTP-version として緩い。

## 設計方針

1. HTTP-version 用の検証では `"HTTP"` の大文字小文字区別の完全一致と major / minor 各 1 桁を要求する。`HTTP/0.9` / `HTTP/1.0` / `HTTP/1.1` / `HTTP/2.0` 等は受理される。`"http/1.1"`、`"HTTP/11.1"`、`"HTTP/1.11"`、`"HTTP/1.01"`、HTTP-name 以外（例: `"FOO/1.0"`）は拒否される。
2. RTSP-version 等の他プロトコル用検証は既存の `is_valid_protocol_version`（`token "/" 1*DIGIT "." 1*DIGIT`）を汎用プロトコルバージョン検証として残す。HTTP 検証と混在させない。
3. HTTP 専用 API である `Request::with_version` / `Response::with_version` / `RequestHead::with_version` / `ResponseHead::with_version`、およびエンコーダー / デコーダーは `is_valid_http_version` を使用する。これにより RTSP 等を HTTP-version として誤って構築・転送する経路を塞ぐ。
4. RTSP 等の他プロトコル構築が必要な場合は、既存の `with_version` を HTTP 専用と位置づけた上で、別途 RTSP 用 API を検討する。本 issue では RTSP 用 API の追加は必須としないが、`is_valid_protocol_version` を直接使う汎用検証経路は存続させ、`with_version` 経由で RTSP を構築していたテスト・PBT は更新・削除する。

## 完了条件

- 以下が HTTP-version として拒否されること:
  - `"http/1.1"`（小文字）
  - `"HTTP/11.1"`（major 多桁）
  - `"HTTP/1.11"`（minor 多桁）
  - `"HTTP/1.01"`（minor 2 桁）
  - `"FOO/1.0"` 等、HTTP-name 以外
  - `"RTSP/1.0"`（HTTP-version としては拒否。RTSP 用 API では維持）
- 以下が HTTP-version として受理されること:
  - `"HTTP/1.1"`、`"HTTP/1.0"`、`"HTTP/2.0"`、`"HTTP/0.9"`
- エンコーダー (`src/encoder.rs`) の `is_valid_version_for_encode` を `is_valid_http_version` に置き換え、VCHAR 緩和を解消すること。
- デコーダー (`src/decoder/request.rs`、`src/decoder/response.rs`) の start-line パース時のバージョン検証を `is_valid_http_version` に置き換えること。ただし RTSP メッセージのデコードを行う場合は別途検討する（本 crate の `RequestDecoder` / `ResponseDecoder` は HTTP 専用を前提とする）。
- RTSP 等の他プロトコル対応が壊れないこと（`is_valid_protocol_version` は存続。ただし `with_version` 経由の RTSP 受理例は HTTP 専用化により更新が必要）。
- 以下のテストを追加・更新すること:
  - `src/validate.rs` インラインテスト: `is_valid_http_version` の受理 / 拒否パターン
  - `tests/test_request.rs`: `Request::with_version` の拒否・受理ケース。既存の RTSP 受理テスト (`test_request_with_version_accepts_rtsp_versions`) と `is_keep_alive` の RTSP/1.1 ケースは更新・削除する
  - `tests/test_response.rs`: `Response::with_version` の拒否・受理ケース
  - `tests/test_decoder/head.rs` および `tests/test_decoder/decode_body.rs`: デコーダー経由の HTTP-version 拒否・受理ケース。`decode_body.rs` の RTSP/FOO Transfer-Encoding 拒否テストは start-line 時点で invalid version となるためエラーメッセージまたはテスト方法を更新する
  - `pbt/tests/prop_response.rs`、`pbt/tests/prop_decoder/request.rs`、`pbt/tests/prop_decoder/response/status_line.rs`: `Response::with_version` やデコーダー入力に RTSP バージョンを生成している箇所を HTTP バージョンに絞る（`is_valid_protocol_version` 自体の網羅性は別途検証してもよい）
- `CHANGES.md` に `[CHANGE]` として以下を記載すること:
  - `Request::with_version` / `Response::with_version` / `RequestHead::with_version` / `ResponseHead::with_version` を HTTP-version 専用に厳格化し、RTSP 等の他プロトコル文字列を拒否するようになったこと
  - RTSP 等の他プロトコル用 API は今後別途検討

## 解決方法

- `src/validate.rs` に `is_valid_http_version` 関数を追加し、RFC 9112 Section 2.3 準拠の検証（`HTTP-name "/" DIGIT "." DIGIT`、ただし `HTTP-name = %x48.54.54.50 ; HTTP`）を行う。
- `is_valid_protocol_version` は RTSP 等を含む汎用プロトコルバージョン検証として存続させ、用途をコメントで明確にする。
- `src/request.rs`、`src/response.rs`、`src/decoder/head.rs`、`src/decoder/request.rs`、`src/decoder/response.rs`、`src/encoder.rs` の HTTP-version 検証を `is_valid_http_version` に置き換える。
- 上記完了条件のテストを追加する。
- `CHANGES.md` を更新する。
