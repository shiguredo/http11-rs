# HTTP-version の構文が RFC 9112 より緩い

- Priority: High
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-http-version-validation-too-lenient
- Polished: {YYYY-MM-DD}

## 目的

HTTP-version の検証を RFC 9112 Section 2.3 に厳密に準拠させ、`"HTTP" "/" DIGIT "." DIGIT` 以外の形式を拒否する。RTSP 等の他プロトコルは別途検証する。

## 優先度根拠

High とする。RFC 9112 では `HTTP-version = HTTP-name "/" DIGIT "." DIGIT`、`HTTP-name = %s"HTTP"` と明記されている。現状は `"http/1.1"`（小文字）、`"HTTP/11.1"`（多桁）、`"FOO/1.0"` 等を HTTP バージョンとして受理しており、送信側で不正なバージョン文字列を生成できてしまう。

## 現状

`src/validate.rs:78-118` の `is_valid_protocol_version` は `token "/" DIGIT+ "." DIGIT+` を許容している。また `src/encoder.rs:19-21` の `is_valid_version_for_encode` も同様に緩い。

## 設計方針

1. HTTP-version 用の検証では `"HTTP"` の完全一致と major / minor 各 1 桁を要求する。
2. RTSP-version 等の他プロトコル用検証は別途用意し、HTTP 検証と混在させない。
3. `Request::with_version` / `Response::with_version` / エンコーダー / デコーダーで共通の HTTP-version 検証を使用する。

## 完了条件

- `"http/1.1"`、`"HTTP/11.1"`、`"FOO/1.0"` 等が HTTP-version として拒否されること。
- `"HTTP/1.1"`、`"HTTP/1.0"` は受理されること。
- RTSP 等の他プロトコル対応が壊れないこと。
- テストが追加されること。

## 解決方法

- `src/validate.rs` に `is_valid_http_version` 関数を追加し、RFC 9112 準拠の検証を行う。
- `is_valid_protocol_version` は RTSP 等を含む汎用検証として残すか、用途を明確にする。
- `src/request.rs`、`src/response.rs`、`src/encoder.rs` で HTTP-version 検証を `is_valid_http_version` に置き換える。
- テストを追加する。
