# tests/test_encoder.rs をディレクトリモジュールに分割する

- Priority: Medium
- Branch: feature/split-test-encoder-into-directory-module
- Created: 2026-05-15
- Model: deepseek v4-pro

## 目的

`tests/test_encoder.rs` が 1323 行と過大であり、CLAUDE.md:100-101 の「テストファイルが長くなった場合はファイル内で `mod` を使って分割すること」に違反する。`tests/test_decoder/` が既に分割済みであるのと対照的。

## 現状

単一ファイルに複数の責務 (encode_request バリデーション、encode_response バリデーション、encode_chunk/chunks、encode_request_headers/response_headers、CRLF/NUL 注入拒否、Content-Length 整合性、CONNECT リクエスト、method/request-target 整合性、桁境界値) が混在している。

## 設計方針

`tests/test_encoder/main.rs` にリネームし、以下のサブモジュールに分割する:
- `headers.rs` — Host ヘッダー/Content-Length 整合性/重複 Content-Length/TE+CL 競合
- `chunk.rs` — encode_chunk/chunks/hex-decimal 境界値
- `connect.rs` — CONNECT リクエストのヘッダー・ボディ許容テスト
- `uri.rs` — absolute-form/authority-form/userinfo/空 host/URN

`test_decoder/` の分割 (main.rs + 5 サブモジュール) を参考にする。

## 完了条件

- `tests/test_encoder.rs` が `tests/test_encoder/main.rs` + サブモジュールに分割されていること
- `cargo test -p shiguredo_http11 --test test_encoder` で全テストが通過すること
