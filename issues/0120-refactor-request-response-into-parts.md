# Request / Response に into_xxx / into_parts がない

- Priority: Medium
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/refactor-request-response-into-parts
- Polished: {YYYY-MM-DD}

## 目的

`Request` / `Response` に対し、`RequestHead` / `ResponseHead` / ボディ / ヘッダー等の所有権移動を可能にする `into_parts` 系メソッドを追加し、API の一貫性を向上させる。

## 優先度根拠

Medium とする。`RequestHead` / `ResponseHead` は `into_xxx` 系メソッドが多数ある一方で、`Request` / `Response` はヘッドを取り出す際にクローンを強いる。プロキシ等の「ヘッドを改変しつつボディはそのまま流す」処理で不要なコピーが発生している。

## 現状

- `src/request.rs` : `Request::head(&self)` のみ。
- `src/response.rs` : `Response::head(&self)` のみ。
- `src/request_head.rs` : `into_method`, `into_uri`, `into_version`, `into_headers` 等あり。
- `src/response_head.rs` : 同上。

## 設計方針

1. `Request::into_parts(self) -> (RequestHead, Body)` を追加する。
2. `Request::into_head(self) -> RequestHead` と `Request::into_body(self) -> Body` も追加する。
3. `Response` についても同様に追加する。
4. 既存の `head(&self)` / `body(&self)` は維持する。

## 完了条件

- `Request::into_parts` / `Request::into_head` / `Request::into_body` が追加されること。
- `Response` についても同様のメソッドが追加されること。
- 既存の API が維持されること。
- テストが追加されること。

## 解決方法

- `src/request.rs` と `src/response.rs` に所有権移動メソッドを追加する。
- `examples/http11_reverse_proxy` 等で `head()` のクローンを `into_head()` に置き換えられる箇所を確認する。
- テストを追加する。
