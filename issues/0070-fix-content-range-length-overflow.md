# ContentRange::length() の整数オーバーフローを修正し、new_bytes() にバリデーションを追加し、fuzz target を拡充する

- Priority: High
- Created: 2026-05-15
- Model: deepseek-v4-pro

## 目的

`ContentRange::length()` が `e - s + 1` を unchecked に計算しており、`e = u64::MAX, s = 0` のとき debug ビルドで panic、release ビルドで wrapping (0 になる) が発生する。

また `new_bytes()` が `start > end` の検証を持たず、`parse()` 経路以外で不正な `ContentRange` が構築可能になっている。

## 優先度根拠

- `ContentRange::new_bytes()` と `ContentRange::length()` は共に `pub` であり、外部利用者が直接呼び出せる API
- `ContentRange::length()` の panic は DoS 経路になり得る
- `parse()` 経路では `start > end` が検証されるため既存 fuzz では到達不能だが、直接構築経路は未防御

## 現状

`src/range.rs:384`:
```rust
pub fn length(&self) -> Option<u64> {
    match (self.start, self.end) {
        (Some(s), Some(e)) => Some(e - s + 1),
        _ => None,
    }
}
```

`src/range.rs:342-349`:
```rust
pub fn new_bytes(start: u64, end: u64, complete_length: Option<u64>) -> Self {
    ContentRange {
        unit: "bytes".to_string(),
        start: Some(start),
        end: Some(end),
        complete_length,
    }
}
```

`fuzz/fuzz_targets/fuzz_range.rs` は `ContentRange::parse()` のみをテストしており、`new_bytes()` の直接構築経路は未到達。

## 設計方針

1. `length()` を `checked_sub` + `checked_add` で安全化する
2. `new_bytes()` に `debug_assert!(start <= end)` を追加する (parse 側と同等の検証)
3. `fuzz_range` に `ContentRange::new_bytes()` の直接構築 + `length()` 呼び出し経路を追加する

## 完了条件

- `length()` が `e = u64::MAX, s = 0` を含む任意の `(start, end)` で panic も wrapping も発生しないこと
- `new_bytes(start, end, _)` が `start > end` のとき `debug_assert!` で検出されること
- fuzz target が `new_bytes()` + `length()` 経路を網羅していること
- `ContentRange::parse()` のラウンドトリップテストが引き続き通過すること
