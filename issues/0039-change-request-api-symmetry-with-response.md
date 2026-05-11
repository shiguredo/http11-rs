# 0039: Request の構築・ヘッダー操作 API を Response と対称化する

Created: 2026-05-12
Model: Opus 4.7

## 概要

`## develop` で Response 側の API は以下に整理されている (issue 0017 / 0021 / 0024 / 0025 の積み重ね):

- 文字列・バイト列受け取りは `impl Into<String>` / `impl Into<Vec<u8>>`
- `add_header` / `set_header` の戻り値は `Result<&mut Self, EncodeError>` (チェイン可)
- `body` / `without_body` / `set_body` / `clear_body` の mutator / builder が揃っている

一方 `Request` 側は:

- `new` / `with_version` が `&str` 固定
- `header` が `&str` 固定、`add_header` / `set_header` の戻り値が `Result<(), EncodeError>` (チェイン不可)
- `body(Vec<u8>)` 固定、`without_body` / `set_body` / `clear_body` が無い

これは canary 中の API 設計の非対称性であり、利用者は Request / Response を使い分けるたびに API 形式の違いに付き合う必要がある。リリース前に対称化する。

## 根拠

- 同じドメイン (HTTP メッセージ構築) で API 形式を分けると、利用者は毎回引数の型・戻り値の型・チェインの可否を覚え直すことになる
- `CHANGES.md` の `## develop` セクションで Response API 変更は `[UPDATE]` / `[CHANGE]` として記載されているが、Request 側の対称変更は未実施
- レビューの致命的指摘 F11 として挙げられている

## 対応方針

### `src/request.rs`

1. `Request::new(method: impl Into<String>, uri: impl Into<String>) -> Result<Self, EncodeError>` に変更
2. `Request::with_version(method: impl Into<String>, uri: impl Into<String>, version: impl Into<String>) -> Result<Self, EncodeError>` に変更
3. `Request::header(self, name: impl Into<String>, value: impl Into<String>) -> Result<Self, EncodeError>` に変更
4. `Request::add_header(&mut self, name: impl Into<String>, value: impl Into<String>) -> Result<&mut Self, EncodeError>` に変更 (チェイン可)
5. `Request::set_header(&mut self, name: impl Into<String>, value: impl Into<String>) -> Result<&mut Self, EncodeError>` に変更 (チェイン可)
6. `Request::body(self, body: impl Into<Vec<u8>>) -> Self` に変更
7. `Request::without_body(self) -> Self` を追加
8. `Request::set_body(&mut self, body: impl Into<Vec<u8>>) -> &mut Self` を追加
9. `Request::clear_body(&mut self) -> &mut Self` を追加

### 既存呼出側の影響

- 既存の `request.add_header(name, value)?;` (戻り値 `Result<(), _>` 前提) は `Result<&mut Self, _>` でも `?` でエラーは伝播し成功値は ; で破棄されるため、コンパイル可能 (response 側と同じ)
- `&str` を受け取っていた箇所は `impl Into<String>` でもそのまま受理されるため、ほとんどの呼出はそのまま動く

### バリデーションのタイミング

`Response` 側で「`.into()` はバリデーション前に実行されるため、無効な入力でも アロケーションが発生する」というトレードオフを採用済み。Request 側も同方針で揃える。

### テスト

- `tests/test_request.rs`: 新シグネチャ (`String` のムーブ、チェイン) を確認するテストを追加
- 既存テストは可能な限り維持。`add_header(...)?;` のような呼出は引き続き動作する

### CHANGES.md

`## develop` のメインに `[CHANGE]` として追記する。

### 破壊的変更

- `Request::new` / `with_version` / `header` 等の API 変更
- `add_header` / `set_header` の戻り値型変更 (`Result<(), _>` → `Result<&mut Self, _>`)
- `body` の引数型変更 (`Vec<u8>` → `impl Into<Vec<u8>>`)
- 新規 builder `without_body` / mutator `set_body` / `clear_body` の追加
- canary 中の破壊的変更として CHANGES.md に `[CHANGE]` / `[UPDATE]` / `[ADD]` で記録
