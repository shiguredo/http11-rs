# 0058: examples 配下の死にコードを削除しお手本としての品質を高める

Created: 2026-05-13
Completed: 2026-05-13
Model: Opus 4.7

## 概要

`/review-code` のレビューで `examples/` 配下に以下の死にコードが検出された:

1. **`examples/http11_server/src/compressor.rs`**: `GzipCompressor` / `BrotliCompressor` / `ZstdCompressor` の構造体・`impl Default`・`impl Compressor` トレイト実装 (約 200 行)
2. **`examples/http11_server_io_uring/src/compressor.rs`**: 同じ構造体・impl ブロック (約 200 行、上と 100% 同一の重複ファイル)
3. **`examples/http11_server/src/main.rs:79-84` `StreamingState::reset()`**: 呼び出し皆無、`#[allow(dead_code)]` で隠蔽
4. **`examples/http11_server_io_uring/src/main.rs:41` `DEFAULT_KEEP_ALIVE_TIMEOUT`**: 「TODO: io_uring でタイムアウト処理を実装する際に使用」コメント付きの未使用 const

AGENTS.md に「サンプルは **お手本** なので性能と堅牢性を両立させること」と明記されており、お手本として死にコードを残すのは害がある。

特に `BrotliCompressor::compress` は単にバッファに溜めるだけで `finish()` で一括圧縮する偽ストリーミング実装 (`Compressor` トレイトのインターフェースを破壊している) で、これを「お手本」として残すのは積極的に有害。

## 根拠

### AGENTS.md 引用

```
## サンプルについて

- サンプルは **お手本** なので性能と堅牢性を両立させること
```

### 検証 (grep 結果)

`examples/http11_server/src/main.rs` の `compressor::` import (L41) は自由関数 `compress_body` / `encoding_header` / `select_encoding` のみ:

```
41:use compressor::{compress_body, encoding_header, select_encoding};
561:    let encoding = accept_encoding.and_then(select_encoding);
672:        match compress_body(body, enc) {
676:                    (compressed, Some(encoding_header(enc)))
```

`GzipCompressor` / `BrotliCompressor` / `ZstdCompressor` 構造体は `main.rs` から一度も参照されていない。`#[allow(dead_code)]` で抑止されていなくても dead code 警告は出ないが、grep で参照箇所が皆無であることを確認済み。

`io_uring/src/main.rs` の `compressor::` import も同様で自由関数のみを使用している。

`StreamingState::reset()` は `examples/http11_server/src/main.rs` の他箇所から呼ばれていない (grep 結果 0 件)。

`DEFAULT_KEEP_ALIVE_TIMEOUT` は `examples/http11_server_io_uring/src/main.rs:41` の宣言のみ、他で参照なし。

### 関連 issue

- 0028 (closed): `Decompressor` トレイト実装を `examples/http11_client` で完成させる。本 issue とは別箇所 (server / io_uring 側) の Compressor 実装が放置されている問題に対応する。

## スコープ

### 削除対象

- `examples/http11_server/src/compressor.rs` の以下 (約 200 行):
  - `pub struct GzipCompressor` とその `impl Default` / `impl std::fmt::Debug` / `impl GzipCompressor` (`new` / `with_quality`) / `impl Compressor for GzipCompressor`
  - `pub struct BrotliCompressor` 関連の同じ要素
  - `pub struct ZstdCompressor` 関連の同じ要素
- `examples/http11_server_io_uring/src/compressor.rs` の同じ struct / impl ブロック (約 200 行)
- `examples/http11_server/src/main.rs:79-84` `StreamingState::reset()` メソッド (`#[allow(dead_code)]` 付き)
- `examples/http11_server_io_uring/src/main.rs:40-42` (`DEFAULT_KEEP_ALIVE_TIMEOUT` 定数と関連 TODO コメント)

### 保持対象

- 両 `compressor.rs` の自由関数 `compress_body` / `select_encoding` / `encoding_header` (main.rs から使用されている)
- 両 `compressor.rs` の `Encoding` enum / `ContentEncoding` 列挙
- `examples/http11_server/src/main.rs` の `StreamingState` 構造体本体と `new()` メソッド (実利用あり)
- `examples/http11_server/src/main.rs:45 DEFAULT_KEEP_ALIVE_TIMEOUT` (L386, L472 で `Duration::from_secs` 引数として実利用)

### 対象外

- 削除後の `compressor.rs` を 1 つの共通モジュールに統合する作業は別 issue で扱う (お手本としての位置づけを保つため examples ごとに独立)
- io_uring 側の他の `unwrap()` / `expect()` 改善は別 issue で扱う

## 対応方針

### コード変更

各削除対象を順に削除する。`use` 文の調整も併せて行う。

### CHANGES.md

`## develop` の `### misc` サブセクションに以下を追加する:

```
- [FIX] `examples/http11_server` / `examples/http11_server_io_uring` の死にコードを削除する
  - `compressor.rs` の `GzipCompressor` / `BrotliCompressor` / `ZstdCompressor` 構造体・`impl Default` / `impl Compressor` トレイト実装 (各約 200 行) を両 example から削除する。両 main.rs は自由関数 `compress_body` / `select_encoding` / `encoding_header` のみを使用しており、struct 群は呼び出し皆無の `#[allow(dead_code)]` 隠蔽コードだった
  - 特に `BrotliCompressor::compress` は単にバッファに溜めるだけで `finish()` で一括圧縮する偽ストリーミング実装で、お手本として残すのは積極的に有害だった
  - `examples/http11_server/src/main.rs::StreamingState::reset()` (`#[allow(dead_code)]` 付き、呼び出し皆無) を削除する
  - `examples/http11_server_io_uring/src/main.rs::DEFAULT_KEEP_ALIVE_TIMEOUT` (未使用 const + 「TODO: io_uring でタイムアウト処理を実装する際に使用」コメント) を削除する。タイムアウト処理を実装する際は新規に const を定義する
  - @voluntas
```

### ブランチ

`feature/fix-remove-examples-dead-code` (`feature/fix-` prefix、機能影響なし、issue 番号を含まない)。

## 受け入れ基準

- 両 `compressor.rs` から `GzipCompressor` / `BrotliCompressor` / `ZstdCompressor` 構造体と関連 impl が削除されている
- 削除後も `compress_body` / `select_encoding` / `encoding_header` が引き続き使用可能
- `examples/http11_server/src/main.rs::StreamingState::reset()` が削除されている
- `examples/http11_server_io_uring/src/main.rs::DEFAULT_KEEP_ALIVE_TIMEOUT` が削除されている
- `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace --all-targets` がすべて PASS
- CHANGES.md `## develop` `### misc` に `[FIX]` エントリが追加されている

## RFC 参照

- 本 issue は RFC 仕様に依存しない (死にコード削除、AGENTS.md「お手本」方針に基づく整理)

## 解決方法

- `examples/http11_server/src/compressor.rs` の `GzipCompressor` / `BrotliCompressor` / `ZstdCompressor` 構造体と関連 impl ブロック (約 300 行) を削除し、自由関数 `select_encoding` / `encoding_header` / `compress_body` のみを残した
- `examples/http11_server_io_uring/src/compressor.rs` から同じ struct / impl ブロックを削除した (両ファイルとも 394 行 → 89 行に縮小)
- `examples/http11_server/src/main.rs::StreamingState::reset()` (`#[allow(dead_code)]` 付き) を削除した
- `examples/http11_server_io_uring/src/main.rs::DEFAULT_KEEP_ALIVE_TIMEOUT` 定数と関連 TODO コメントを削除した
- `CHANGES.md` の `## develop` `### misc` に `[FIX]` エントリを追加した
