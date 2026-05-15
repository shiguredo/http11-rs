# ResponseDecoder::status_code フィールドのデッドストアを削除する

- Priority: Low
- Created: 2026-05-15
- Model: deepseek-v4-pro

## 目的

`ResponseDecoder::status_code` フィールドは 4 箇所で書き込まれるが、コード内で一度も読み取られないデッドストアである。静的解析の誤検出を誘発し、リファクタリング時に混乱を招く。

## 現状

`src/decoder/response.rs:60` (フィールド定義)、`response.rs:91,110,301,527,609,859` (書き込み箇所)。全書き込みに対して読み取りが存在しない。

## 設計方針

フィールドを削除し、`reset()` / `decode_headers()` / `decode()` のゼロクリア代入を除去する。`determine_body_kind` には引数で status_code が渡されており、フィールド経由の参照は不要。

## 完了条件

- `status_code` フィールドが削除されていること
- 全テストが通過すること
