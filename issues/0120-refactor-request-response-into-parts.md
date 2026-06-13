# Request / Response に into_xxx / into_parts がない

- Priority: Medium
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/refactor-request-response-into-parts
- Polished: 2026-06-13

## 目的

`Request` / `Response` に対し、`RequestHead` / `ResponseHead` / ボディ等の所有権移動を可能にする `into_parts` 系メソッドを追加し、`RequestHead` / `ResponseHead` との API 一貫性を向上させる。

## 優先度根拠

Medium とする。`RequestHead` / `ResponseHead` は `into_xxx` 系メソッドが多数ある一方で、`Request` / `Response` はヘッドを取り出す際に各フィールドをクローンする必要がある。プロキシ等の「ヘッドを改変しつつボディはそのまま流す」処理で不要なコピーが発生している。

## 現状

- `src/request.rs:327` : `Request::body_bytes(&self) -> Option<&[u8]>` のみ。`RequestHead` を所有権移動で取り出すメソッドは存在しない。
- `src/response.rs:409-421` : `Response::body_bytes(&self) -> Option<&[u8]>` と `Response::is_body_omitted(&self) -> bool` のみ。`ResponseHead` を所有権移動で取り出すメソッドは存在しない。
- `src/decoder/head.rs:272-302` : `RequestHead` に `into_method`, `into_uri`, `into_version`, `into_headers`, `into_parts` 等の所有権移動メソッドがある。
- `src/decoder/head.rs:480-509` : `ResponseHead` に `into_version`, `into_reason_phrase`, `into_headers`, `into_parts` 等の所有権移動メソッドがある。
- `src/decoder/head.rs:314-337` : `RequestHead::from_validated_parts` により、バリデーション済みの所有値から `RequestHead` を zero-copy で構築できる。
- `src/decoder/head.rs:519-546` : `ResponseHead::from_validated_parts` により、バリデーション済みの所有値から `ResponseHead` を zero-copy で構築できる。

## 設計方針

1. `Request::into_parts(self) -> (RequestHead, Option<Vec<u8>>)` を追加する。
   - `RequestHead::from_validated_parts` を内部で利用し、フィールドをムーブする。`Request` は構築時にバリデーション済みなので再検証は不要。
2. `Request::into_head(self) -> RequestHead` を追加する。
   - ボディは破棄される。
3. `Request::into_body(self) -> Option<Vec<u8>>` を追加する。
   - ヘッドは破棄される。
4. `Response` についても同様に `into_parts`, `into_head`, `into_body` を追加する。
   - 戻り値は `(ResponseHead, Option<Vec<u8>>)` とする。
   - `Response` は `omit_body: bool` という送信専用フラグを持つが、`ResponseHead` に該当フィールドはないため、`into_parts` / `into_head` では `omit_body` は失われる。必要な場合は事前に `is_body_omitted()` で取得しておくこと。
5. 既存の `body_bytes(&self)` およびその他の参照取得 API は維持する。

## 完了条件

- `src/request.rs` に以下が追加されること:
  - `Request::into_parts(self) -> (RequestHead, Option<Vec<u8>>)`
  - `Request::into_head(self) -> RequestHead`
  - `Request::into_body(self) -> Option<Vec<u8>>`
- `src/response.rs` に以下が追加されること:
  - `Response::into_parts(self) -> (ResponseHead, Option<Vec<u8>>)`
  - `Response::into_head(self) -> ResponseHead`
  - `Response::into_body(self) -> Option<Vec<u8>>`
- 新規メソッドは所有権移動のみを行い、フィールドのクローンを発生させないこと。
- 既存の `body_bytes(&self)` 等の参照取得 API が維持されること。
- `tests/test_request.rs` / `tests/test_response.rs` に以下のテストを追加すること:
  - `into_parts` でヘッドとボディが正しく分離されること
  - `into_head` でヘッドのみが取得でき、ボディは破棄されること
  - `into_body` でボディのみが取得でき、ヘッドは破棄されること
  - ボディが `None` / `Some(vec![])` / `Some(data)` の各パターン
  - `Response::into_parts` / `into_head` 後、`ResponseHead` から `omit_body` の値を取得できないこと（失われること）。必要に応じて `into_parts` 呼び出し前に `is_body_omitted()` で取得しておくこと
- `CHANGES.md` の `## develop` に以下を追加すること。
  - `[ADD] Request / Response に RequestHead / ResponseHead / ボディを所有権移動で取り出す into_parts() / into_head() / into_body() を追加する`
- ドキュメントコメントは日本語で記述すること。

## 解決方法

- `src/request.rs` と `src/response.rs` に所有権移動メソッドを追加する。
- 内部では `RequestHead::from_validated_parts` / `ResponseHead::from_validated_parts` を使用し、zero-copy でフィールドを移動する。
- `examples/http11_reverse_proxy` では現状 `RequestDecoder::decode_headers` / `ResponseDecoder::decode_headers` から直接 `RequestHead` / `ResponseHead` を取得して `into_parts()` しているため、本追加メソッドを直接使用する箇所はないが、他の例や内部コードで `Request` / `Response` からヘッドを取り出す必要がある箇所があれば適用を検討する。
- テストを追加する。
