# 0057: `RequestHead` / `ResponseHead` に inherent な `version()` / `headers()` を追加し CHANGES.md 記載との乖離を解消する

Created: 2026-05-13
Completed: 2026-05-13
Model: Opus 4.7

## 概要

CHANGES.md `## develop` の `[CHANGE] RequestHead / ResponseHead の全フィールドを非公開化し、構築時バリデーション付きの公開 API を追加する` エントリ (現在 L82-89) には以下のように明記されている:

> 読み取り専用アクセサ: `method()` / `uri()` / `version()` / `headers()` / `status_code()` / `reason_phrase()` / `status_class()` を追加

しかし実際の実装 (`src/decoder/head.rs`) では、`RequestHead` の inherent method は `method()` / `uri()` のみ (L228-237)、`ResponseHead` の inherent method は `status_code()` / `reason_phrase()` / `status_class()` のみ (L391-412)。`version()` / `headers()` は `HttpHead` トレイト経由でしか呼べない。

利用者がアクセサを呼ぶには `use shiguredo_http11::HttpHead;` を明示的に import する必要があり、CHANGES.md の文面と乖離している。

CHANGES.md の記述に合わせて inherent な `version()` / `headers()` を追加する。

## 根拠

### CHANGES.md 引用

`CHANGES.md` `## develop` セクション [CHANGE] `RequestHead` / `ResponseHead` の全フィールドを非公開化:

> 読み取り専用アクセサ: `method()` / `uri()` / `version()` / `headers()` / `status_code()` / `reason_phrase()` / `status_class()` を追加

### Request / Response 側との対称性

`Request` / `Response` は既に inherent な `version()` / `body_bytes()` 等を持っており (`src/request.rs:327`, `src/response.rs:394`)、`RequestHead` / `ResponseHead` だけが trait 経由でしか version()/headers() を呼べないのは対称性が崩れている。

### 関連 issue

- 0031 (closed): `RequestHead` / `ResponseHead` フィールド非公開化と公開 API 追加。本 issue はその追加 API のうち実装漏れだった `version()` / `headers()` を補完する。

## スコープ

### 対象

- `src/decoder/head.rs::RequestHead` に inherent な `pub fn version(&self) -> &str` を追加
- `src/decoder/head.rs::RequestHead` に inherent な `pub fn headers(&self) -> &[(String, String)]` を追加
- `src/decoder/head.rs::ResponseHead` に同じく `version()` / `headers()` を追加
- 各メソッドに `#[must_use]` を付与する (他の getter と対称)
- doc コメントを RFC 節番号付きで記載する

### 対象外

- `HttpHead` トレイトの実装は変更しない (trait method として引き続き呼び出し可能)
- `Request` / `Response` 側は対象外 (既に inherent method が存在する)

## 対応方針

### コード変更

`impl RequestHead { ... }` ブロック (`src/decoder/head.rs:173-294`) の中、`uri()` メソッドの直後に追加:

```rust
/// HTTP プロトコルバージョンを取得 (例: `"HTTP/1.1"`)。
///
/// RFC 9112 Section 2.3 HTTP-version。
#[must_use]
pub fn version(&self) -> &str {
    &self.version
}

/// ヘッダーリストを取得。
///
/// RFC 9110 Section 5。順序は受信順を保持する。
#[must_use]
pub fn headers(&self) -> &[(String, String)] {
    &self.headers
}
```

`impl ResponseHead { ... }` ブロック (`src/decoder/head.rs:328-470`) の中、`status_class()` メソッドの直後に同じものを追加。

### `HttpHead` トレイト実装との関係

Rust の method resolution は inherent method を優先するため、`head.version()` の呼び出しは inherent 経由になる。`HttpHead` トレイトの version()/headers() は default 実装ではなく abstract method で、各型の `impl HttpHead for ...` で実装されている (L296-304 / L472-480)。

inherent method と trait method は本質的に同じロジック (`&self.version` を返す) なので、どちらの経路でも結果は同じ。

`HttpHead` の他のデフォルト実装メソッド (`content_length` / `is_keep_alive` / `is_chunked` / `connection` 等) は `self.version()` / `self.headers()` を内部で呼ぶが、Rust の method resolution により inherent method が呼ばれる。inherent と trait method のロジックが同一なので動作は変わらない。

### テスト

既存の `tests/test_decoder.rs` / `pbt/tests/prop_decoder/` で `RequestHead::version()` / `ResponseHead::version()` を直接呼ぶテストが既に動作している (trait 経由)。inherent method 追加後も同じテストが PASS することを確認する。

新規テスト追加は不要 (機能変更ではないため)。

### CHANGES.md

`## develop` の `### misc` サブセクションに `[ADD]` として追加する:

```
- [ADD] `RequestHead` / `ResponseHead` に inherent な `version()` / `headers()` アクセサを追加する
  - 0031 で記載していた「読み取り専用アクセサ: `method()` / `uri()` / `version()` / `headers()` / ...」のうち `version()` / `headers()` が実装漏れで `HttpHead` トレイト経由でしか呼べない状態だった
  - inherent method 追加により `use shiguredo_http11::HttpHead;` の import なしで `head.version()` / `head.headers()` が呼べるようになる (Request / Response 側と対称)
  - `HttpHead` トレイト実装は変更しないため trait 経由のアクセスも引き続き可能
  - @voluntas
```

並び順は misc の `[UPDATE]` 群の次、既存 `[ADD]` 群の中。

### ブランチ

`feature/add-head-inherent-version-headers` (`feature/add-` prefix、後方互換あり、issue 番号を含まない)。

## 受け入れ基準

- `RequestHead` に inherent な `version()` / `headers()` が追加されている
- `ResponseHead` に inherent な `version()` / `headers()` が追加されている
- 両メソッドに `#[must_use]` と doc コメントが付与されている
- `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace --all-targets` がすべて PASS
- CHANGES.md `## develop` `### misc` に `[ADD]` エントリが追加されている
- `HttpHead` トレイト実装は変更されていない (trait method による呼び出しが引き続き動作する)

## RFC 参照

- RFC 9112 Section 2.3 (HTTP-version、`refs/rfc9112.txt`)
- RFC 9110 Section 5 (Fields、`refs/rfc9110.txt`)

## 解決方法

- `src/decoder/head.rs::RequestHead` の `uri()` の直後に inherent な `version()` / `headers()` を追加した
- `src/decoder/head.rs::ResponseHead` の `status_class()` の直後に inherent な `version()` / `headers()` を追加した
- 両メソッドに `#[must_use]` と RFC 節番号付き doc コメントを付与した
- `HttpHead` トレイト実装は変更しなかった (trait 経由のアクセスも引き続き動作する)
- `CHANGES.md` の `## develop` `### misc` に `[ADD]` エントリを追加した
