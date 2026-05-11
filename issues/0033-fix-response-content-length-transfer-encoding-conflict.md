# 0033: レスポンス受信側で Transfer-Encoding と Content-Length の同時受信をエラー化する

Created: 2026-05-12
Model: Opus 4.7

## 概要

`src/decoder/body.rs::resolve_body_headers_for_response` は、レスポンスが Transfer-Encoding と Content-Length を**両方**含む場合、現状は silent に Transfer-Encoding を優先して Content-Length を無視している。コメントには「警告ログを出すべきとあるが、本実装では無視のみ」と記されている。

一方 `resolve_body_headers_for_request` は同条件でエラーを返す。同じレスポンス受信経路で挙動が分裂しており、HTTP Request Smuggling (CWE-444) / Response Splitting (CWE-113) の検出機会を逃している。

RFC 9112 Section 6.3 (3) は本ケースを次のように扱う:

> If a message is received with both a Transfer-Encoding and a Content-Length header field, the Transfer-Encoding overrides the Content-Length. Such a message might indicate an attempt to perform request smuggling (Section 11.2) or response splitting (Section 11.1) and **ought to be handled as an error**.

さらに RFC 9112 Section 6.1 lines 880-884:

> A server MAY reject a request that contains both Content-Length and Transfer-Encoding ... Regardless, the server **MUST close the connection** after responding to such a request to avoid the potential attacks.

## 根拠

### 攻撃シナリオ

1. プロキシ越しのレスポンスで、上流が `Transfer-Encoding: chunked\r\nContent-Length: 100\r\n` を含む
2. 本実装は silent に Transfer-Encoding を優先するため、上位層は smuggling/splitting 攻撃を検知できない
3. 仮にプロキシが同じ接続上で複数レスポンスを連続で扱う場合、Content-Length と chunked の境界解釈差で smuggling が成立する経路を作る

### リクエスト経路との不整合

- リクエスト経路 (`resolve_body_headers_for_request`): 両方含む場合は `Error::InvalidData("invalid message: both Transfer-Encoding and Content-Length")` を返す
- レスポンス経路 (`resolve_body_headers_for_response`): silent に TE を優先

同じ仕様 (RFC 9112 §6.3 (3)) の運用を、送受信方向で分けている合理的根拠はない。

## 対応方針

### `src/decoder/body.rs::resolve_body_headers_for_response`

- 両方含まれる場合は `Error::InvalidData("invalid message: both Transfer-Encoding and Content-Length")` を返す (リクエスト経路と同一エラー文言)
- 旧コメント「警告ログを出すべきとあるが、本実装では無視のみ」を撤去し、RFC 9112 §6.3 (3) の MUST close 文面を引用する

### テスト

- `tests/test_decoder.rs`: TE + CL 両方を含むレスポンスを `decode_headers` で受信した場合に `Error::InvalidData` が返ることを確認
- 既存 PBT `prop_decoder/response.rs` で TE + CL 両方を含むテストがないかを確認し、必要に応じて期待値を更新

### CHANGES.md

`## develop` のメインに `[CHANGE]` として追記する。受信側の挙動変更で後方互換性なし。

### 破壊的変更

- 旧挙動 (silent に TE 優先) に依存していたユーザーは `Error::InvalidData` を受け取るようになる
- canary リリース中なので破壊的変更は許容範囲
- 既存挙動でも上位層は smuggling を検知できなかったため、本変更は実害なしか改善のみ
