# 0011: #[macro_use] extern crate alloc の #[macro_use] を削除する

Created: 2026-04-28
Model: Kimi 2.6 / GPT 5.5 / Composer 2 Fast

## 概要

`src/lib.rs` の `#[macro_use] extern crate alloc;` から `#[macro_use]` を削除する。`extern crate alloc;` 自体は `no_std` crate として必要だが、`#[macro_use]` は不要。

ただし、`src/range.rs` で `vec!` macro を使っているため、先に `alloc::vec!` への置換が必要。

## 根拠

現状の `src/lib.rs`:

```rust
#[macro_use]
extern crate alloc;
```

ライブラリ本体の通常ビルド（doc comment / `#[cfg(test)]` を除く）で `vec!` macro を使っている箇所は `src/range.rs` のみ:

```rust
// src/range.rs L454-L461
impl AcceptRanges {
    pub fn bytes() -> Self {
        Self {
            units: vec!["bytes".to_string()],
        }
    }

    pub fn none() -> Self {
        Self {
            units: vec!["none".to_string()],
        }
    }
}
```

`src/range.rs` には既に `use alloc::vec::Vec;` があるが、`vec!` macro は `#[macro_use] extern crate alloc;` で prelude 的に使えている。`#[macro_use]` を削除すると、`vec!` macro がスコープから消える。

## 前提

本 issue を実施する前に、`src/range.rs` の `vec!` を `alloc::vec!` に置換する必要がある。

## 対象ファイルと変更点

### `src/range.rs`

`vec!` を `alloc::vec!` に置換:

```rust
// Before
units: vec!["bytes".to_string()],
// After
units: alloc::vec!["bytes".to_string()],
```

### `src/lib.rs`

```rust
// Before
#[macro_use]
extern crate alloc;

// After
extern crate alloc;
```

## 影響範囲

- `src/range.rs` と `src/lib.rs` のみ。
- `no_std` ビルドに影響しない（`extern crate alloc;` は残る）。

## 検証

- `make fmt && make clippy && make check && make test` を通す。
- `cargo build --no-default-features`（no_std ビルド）が成功することを確認する。
- `rg 'vec!\[' src/ --type rust` でライブラリ本体で `vec!` macro が使われていないことを確認する。ただしこのコマンドは doc comment 内や `#[cfg(test)]` 内も含むため、出力結果から手動で除外する。または `src/` 内の `.rs` ファイルを個別に確認する。
