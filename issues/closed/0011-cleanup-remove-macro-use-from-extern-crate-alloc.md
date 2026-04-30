# 0011: `#[macro_use] extern crate alloc` の `#[macro_use]` を削除する

Created: 2026-04-28
Completed: 2026-04-30
Model: Kimi 2.6 / GPT 5.5 / Composer 2 Fast

## 概要

`src/lib.rs` の `#[macro_use] extern crate alloc;` から `#[macro_use]` を削除する。`extern crate alloc;` 自体は `no_std` crate として必要だが、`#[macro_use]` は不要。

`#[macro_use]` は `alloc` crate のマクロ（`vec!`, `format!` 等）を prelude 的にスコープに入れるが、`no_std` 環境では各ファイルで明示的に `alloc::vec!` / `alloc::format!` を使う方が意図が明確となる。

## 根拠

現状の `src/lib.rs`:

```rust
#[macro_use]
extern crate alloc;
```

`#[macro_use]` を削除すると、`alloc` のマクロがスコープから消える。ライブラリ本体の通常ビルド（doc comment / `#[cfg(test)]` を除く）で `vec!` / `format!` マクロを使っている箇所が複数ファイルに存在するため、これらを `alloc::vec!` / `alloc::format!` に置換する必要がある。

## 対象ファイルと変更点

### `src/lib.rs`

```rust
// Before
#[macro_use]
extern crate alloc;

// After
extern crate alloc;
```

### `src/range.rs`

`vec!` を `alloc::vec!` に置換:

```rust
// Before
units: vec!["bytes".to_string()],
// After
units: alloc::vec!["bytes".to_string()],
```

### `format!` を使っているファイル

以下のファイルで `format!` を `alloc::format!` に置換:

- `src/accept.rs`
- `src/auth.rs`
- `src/cache.rs`
- `src/content_disposition.rs`
- `src/content_type.rs`
- `src/decoder/body.rs`
- `src/decoder/request.rs`
- `src/decoder/response.rs`
- `src/encoder.rs`
- `src/multipart.rs`
- `src/uri.rs`

`#[cfg(test)]` 内や doc comment 内の `format!` / `vec!` は `std` 環境でビルドされるため変更不要。

## 影響範囲

- `src/lib.rs` と `format!` / `vec!` を使っているファイル群。
- `no_std` ビルドに影響しない（`extern crate alloc;` は残る）。

## 検証

- `make fmt && make clippy && make check && make test` を通す。
- `cargo build --no-default-features`（no_std ビルド）が成功することを確認する。

## 解決方法

`src/lib.rs` の `#[macro_use] extern crate alloc;` を `extern crate alloc;` に変更し、`vec!` / `format!` を使っていた通常コードを全て `alloc::vec!` / `alloc::format!` に置換した。`#[cfg(test)]` 内や doc comment 内は `std` 環境でビルドされるため変更不要。
