# 0007: HttpHead トレイトを Request/Response に実装して重複を委譲に置き換える

Created: 2026-04-28
Completed: 2026-04-30
Model: Kimi 2.6 / GPT 5.5 / Composer 2 Fast

## 概要

`Request` と `Response` に個別に実装されている `get_header`、`get_headers`、`has_header`、`connection`、`is_keep_alive`、`is_chunked` メソッドを、`HttpHead` トレイトのデフォルト実装に委譲する。`content_length()` は 0006 と連携して `Option<u64>` に変更するため破壊的変更。`get_header` 等はシグネチャを維持したまま trait メソッドに委譲する。

## 根拠

現状 `src/request.rs`（L68-L146）と `src/response.rs`（L86-L184）に 120 行以上の同一ロジックが重複しており、`src/decoder/head.rs` の `HttpHead` トレイトでもほぼ同じ実装がデフォルトメソッドとして提供されている。重複により以下の問題がある。

- 将来仕様解釈の修正（例: `is_keep_alive` の Connection ヘッダー解析ロジック）があった場合、3 箇所すべてを修正する必要があり、漏れリスクがある。
- `Request`/`Response` の実装と `HttpHead` の実装で細部が異なる（例: `HttpHead::is_keep_alive` は `get_headers("Connection")` を使用するが、`Request`/`Response` は生のヘッダーイテレーションを行っている）。実装スタイルの差異により、将来的に挙動が分岐するリスクがある。

なお `RequestHead` と `ResponseHead` は既に `HttpHead` を実装済み（`src/decoder/head.rs` L115-L145）であり、`Request`/`Response` も同様に実装すればよい。

## 前提

本 issue は 0006（Content-Length の型変更）と同時または直後に実施すべき。先に `HttpHead::content_length()` のみ `Option<usize>` のまま trait 実装を行うと、悪い API を共通化するだけになるため。

## 対象ファイルと変更点

### `src/decoder/head.rs`

- `content_length()` の戻り型を `Option<u64>` に変更（0006 と連携）。

### `src/request.rs`

1. `impl HttpHead for Request` を追加:
   ```rust
   impl HttpHead for Request {
       fn version(&self) -> &str {
           &self.version
       }

       fn headers(&self) -> &[(String, String)] {
           &self.headers
       }
   }
   ```

2. 既存の inherent method を trait メソッドに委譲する薄いラッパーに変更:
   ```rust
   pub fn get_header(&self, name: &str) -> Option<&str> {
       HttpHead::get_header(self, name)
   }
   ```

   `get_headers`、`has_header`、`connection`、`is_keep_alive`、`is_chunked` も同様。

   `content_length` も同様だが、戻り型は `Option<u64>` に変更（0006 と連携）。

### `src/response.rs`

`Request` と同様に `impl HttpHead for Response` を追加し、inherent method をラッパー化する。

### `src/lib.rs`

`HttpHead` トレイトをクレート外で直接使うには公開が必要だが、現状 `lib.rs` で `decoder` モジュール経由に re-export 済みなので、この issue 用の追加変更は不要。

現状の `lib.rs`:
```rust
pub use decoder::{ BodyKind, BodyProgress, HttpHead, ... };
```

## 影響範囲

- `get_header`、`get_headers`、`has_header`、`connection`、`is_keep_alive`、`is_chunked` の公開 API シグネチャは維持されるため、呼び出し側への影響はなし。
- `content_length()` の戻り型変更（`Option<usize>` → `Option<u64>`）は破壊的変更（0006 と同時対応）。

## 検証

- `make fmt && make clippy && make check && make test` を通す。
- `Request`/`Response` の `get_header`、`is_keep_alive`、`content_length`、`is_chunked` の単体テストがそのまま緑であることを確認する。
- `HttpHead` トレイトのデフォルト実装を使うことで挙動が変わらないことを確認する。
- 以下の呼び出しが等価であることを確認する:
  ```rust
  let req = Request::new("GET", "/");
  assert_eq!(req.get_header("Host"), HttpHead::get_header(&req, "Host"));
  ```

## 解決方法

- `src/request.rs` に `impl HttpHead for Request` を追加し、`version()` と `headers()` のみ実装した。
- `src/request.rs` の `get_header` / `get_headers` / `has_header` / `connection` / `is_keep_alive` / `content_length` / `is_chunked` を `HttpHead` トレイトメソッドに委譲する薄いラッパーに変更した。
- `src/response.rs` に `impl HttpHead for Response` を追加し、同様にラッパー化した。
- `Response` の `is_success` / `is_redirect` / `is_client_error` / `is_server_error` / `is_informational` は `HttpHead` に含まれないためそのまま残した。
- 既存のテスト（lib 283 + integration 283 + PBT 448）がすべて通過することを確認した。
