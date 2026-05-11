# 0031: RequestHead / ResponseHead のフィールドを非公開化する

Created: 2026-05-11
Model: Opus 4.7

## 概要

`src/decoder/head.rs` で定義されている `RequestHead` / `ResponseHead` は全フィールドが `pub` であり、外部から構造体リテラルで自由に構築できる。具体的には:

```rust
ResponseHead {
    version: "BAD".to_string(),
    status_code: 999,
    reason_phrase: "...".to_string(),
    headers: vec![("Bad\r\n".to_string(), "X".to_string())],
}
```

このような不正値を持つ `ResponseHead` に対して `status_class()` を呼ぶと、`StatusClass::from_status_code(999)` が `None` を返し、`.expect(...)` で panic する (`src/decoder/head.rs:198`)。

ライブラリの公開 API がユーザー操作で panic することは原則として避けるべきで、現状の `ResponseHead::status_class` はその原則を破っている。`src/decoder/head.rs:188-193` のコメントもこの問題を自認しており、将来の issue で非公開化することが予告されている。

`Request` / `Response` は既に `0017` / `0025` で同等の非公開化＋バリデート付き構築 API が完了している。`RequestHead` / `ResponseHead` も対称な扱いにする。

## 根拠

### 問題: status_class の panic 経路

`ResponseHead { status_code: 999, .. }` のような構築が外部から可能なため、`status_class()` が panic する経路がユーザーコードから到達可能。`HttpHead` トレイトの doc が「ライブラリは panic しない」ことを暗黙に保証している前提と衝突する。

### Request / Response との対称性

- `Request` (0025): フィールド非公開、`new` / `with_version` / `header` 等で構築時バリデーション、`method()` / `uri()` 等のアクセサ
- `Response` (0017): 同上、`status_code()` / `reason_phrase()` 等のアクセサ
- `RequestHead` / `ResponseHead`: 全フィールド `pub` のまま

これは設計の非対称性であり、整理する。

### CHANGES.md の `## develop` 整合性

`## develop` には既に `[CHANGE] Response の全フィールドを非公開化し、構築時バリデーションを追加する` と `[CHANGE] Request の全フィールドを非公開化し、構築時バリデーションを追加する` が記載されているが、これは `Request` / `Response` (送信用) の話で、`RequestHead` / `ResponseHead` (受信用) は別。本 issue で対称化する。

## 対応方針

### `src/decoder/head.rs`

- `RequestHead` / `ResponseHead` のフィールドを `pub(crate)` に降格する
- `#[non_exhaustive]` を付与する (将来のフィールド追加を非破壊的に扱う)
- 公開アクセサを追加する:
  - `RequestHead`: `pub fn method(&self) -> &str`, `pub fn uri(&self) -> &str`, `pub fn version(&self) -> &str`, `pub fn headers(&self) -> &[(String, String)]`
  - `ResponseHead`: `pub fn version(&self) -> &str`, `pub fn status_code(&self) -> u16`, `pub fn reason_phrase(&self) -> &str`, `pub fn headers(&self) -> &[(String, String)]`
- decoder 内部からの構築用に `pub(crate) fn from_validated_parts(...)` を新設する (バリデーションスキップ、信頼済み入力前提)
- HttpHead トレイトの `version()` / `headers()` と inherent メソッドの `version()` / `headers()` は名前衝突するが、inherent が優先されるので問題ない (Request / Response でも同パターン)
- `status_class()` の `.expect(...)` は維持できる: フィールド非公開化により decoder 経由でしか構築できなくなり、`status_code` の 100..=599 不変式が保証される

### `src/decoder/request.rs` / `src/decoder/response.rs`

- `RequestHead { ... }` / `ResponseHead { ... }` の構造体リテラルを `RequestHead::from_validated_parts(...)` / `ResponseHead::from_validated_parts(...)` に書き換える

### テスト

書き換えが必要なファイル:

- `tests/test_decoder.rs`: `head.method` / `head.status_code` 等のフィールドアクセスをアクセサ呼び出し (`head.method()`, `head.status_code()`) に変更
- `tests/test_request_target.rs`: 同上
- `pbt/tests/prop_request.rs` / `prop_response.rs`: フィールドアクセス → アクセサ
- `pbt/tests/prop_decoder/head.rs`: 17 個の `ResponseHead { ... }` 構造体リテラルを `ResponseHead::from_validated_parts(...)` の公開バージョン (= `new` 系) に置き換える必要がある

PBT は外部クレートのため `pub(crate)` の `from_validated_parts` は呼べない。代わりに以下のいずれかを提供:

選択肢 A: テスト用に `ResponseHead::new(version, status_code, reason_phrase, headers) -> Result<Self, EncodeError>` を `pub` で提供。バリデート付き構築 (status_code は 100..=599、version は valid、reason_phrase は valid、headers は valid)。

選択肢 B: `RequestHead` / `ResponseHead` を内部的に builder で構築できる API を `pub` 化する。

選択肢 A の方がシンプル。`Response::new` / `Request::new` は送信用のため、受信用 `ResponseHead::new` / `RequestHead::new` は引数の意味と用途が違うが、命名規則として `new` で揃える。

### CHANGES.md

`## develop` のメインに `[CHANGE]` として追記する。

### 破壊的変更

- `RequestHead.method` / `RequestHead.uri` / `RequestHead.version` / `RequestHead.headers` の直接アクセスが不可になる (アクセサ経由必須)
- `ResponseHead.version` / `ResponseHead.status_code` / `ResponseHead.reason_phrase` / `ResponseHead.headers` の直接アクセスが不可になる
- 構造体リテラル構築 (`ResponseHead { ... }`) は外部から不可になる
- `#[non_exhaustive]` の付与
- canary リリース中なので破壊的変更は許容範囲
