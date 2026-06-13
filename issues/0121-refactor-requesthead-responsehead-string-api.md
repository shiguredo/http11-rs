# RequestHead / ResponseHead の文字列系 API が String 非対応

- Priority: Medium
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/refactor-requesthead-responsehead-string-api
- Polished: 2026-06-13

## 目的

`RequestHead` / `ResponseHead` の文字列を引数に取るコンストラクタやセッターが `&str` 固定になっているため、`String` や `Cow<'_, str>` を直接渡せるようにし、不要なクローンを削減する。

## 優先度根拠

Medium とする。`ResponseHead::new` / `with_version` の `reason_phrase` や `RequestHead::with_version` の `uri` / `version` 等が `&str` 固定で、動的に構築した `String` を渡す際に `.to_string()` や `.clone()` が発生している。メッセージ生成のホットパスでは軽微ながらコストが積み上がる。

## 現状

`RequestHead` / `ResponseHead` は `src/decoder/head.rs` に定義されている。文字列引数が `&str` 固定になっている API は以下の通り。

- `src/decoder/head.rs:182-192` : `RequestHead::new(method, uri: &str)`
- `src/decoder/head.rs:194-215` : `RequestHead::with_version(method, uri: &str, version: &str)`
- `src/decoder/head.rs:217-225` : `RequestHead::header(self, name, value: &str)`
- `src/decoder/head.rs:227-242` : `RequestHead::add_header(&mut self, name, value: &str)`
- `src/decoder/head.rs:372-382` : `ResponseHead::new(status_code: u16, reason_phrase: &str)`
- `src/decoder/head.rs:384-410` : `ResponseHead::with_version(version: &str, status_code: u16, reason_phrase: &str)`
- `src/decoder/head.rs:412-420` : `ResponseHead::header(self, name, value: &str)`
- `src/decoder/head.rs:422-437` : `ResponseHead::add_header(&mut self, name, value: &str)`

一方、同じく公開 API の `Request` (`src/request.rs:72-136`) / `Response` (`src/response.rs:86-170`) では、これらの文字列引数は既に `impl Into<String>` を採用しており、`RequestHead` / `ResponseHead` との間で API 一貫性が欠けている。

## 設計方針

1. `Request` / `Response` と同じく `impl Into<String>` を受け入れる形式に統一する。
2. 既存の `&str` 呼び出しは `.into()` による暗黙変換でそのままコンパイルできるようにする。
3. `String` を渡した場合はムーブによりゼロコピーで受け取る。`Cow<'_, str>` も `Into<String>` を実装しているため利用可能。
4. バリデーション前に `.into()` して所有権を取得するため、無効な入力でもアロケーションが発生する。これは `Request` / `Response` 側と同じトレードオフである。
5. `Cow` 借用時のゼロコピーバリデーションは本 issue では行わない。`impl Into<String>` に統一することで API 一貫性を優先する。

## 完了条件

- `RequestHead::new` の `uri`、および `RequestHead::with_version` の `uri` / `version` を `impl Into<String>` に変更すること。
- `ResponseHead::new` の `reason_phrase`、および `ResponseHead::with_version` の `version` / `reason_phrase` を `impl Into<String>` に変更すること。
- `RequestHead::header` / `add_header` の `value` を `impl Into<String>` に変更すること。
- `ResponseHead::header` / `add_header` の `value` を `impl Into<String>` に変更すること。
- 以下の呼び出しがすべてコンパイルし、動作すること。
  - 既存の `&str` リテラルからの呼び出し
  - 所有済み `String` からの呼び出し
  - `Cow<'_, str>` からの呼び出し
- `String` を渡した際に不要なクローンが発生しないこと（ムーブで受け取る）。
- `tests/test_decoder/head.rs` に、上記の型からの構築テストを追加すること。
- `CHANGES.md` に本変更を記載すること。

## 解決方法

- `src/decoder/head.rs` の該当 API の文字列引数を `&str` から `impl Into<String>` に変更する。
- 各メソッド内で `.into()` による変換後、既存のバリデーション関数を適用する。
- 影響範囲は `RequestHead` / `ResponseHead` のみで、`examples/` では decoder から取得した head の getter のみを使用しているため修正は不要。
- テストを追加し、`String` / `Cow<'_, str>` / `&str` の各経路で構築と getter 取得が正しく動作することを検証する。
