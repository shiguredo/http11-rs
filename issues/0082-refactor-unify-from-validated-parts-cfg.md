# from_validated_parts の #[cfg(debug_assertions)] 複製を単一実装に統合する

- Priority: Medium
- Created: 2026-05-15
- Model: deepseek-v4-pro

## 目的

`RequestHead::from_validated_parts` と `ResponseHead::from_validated_parts` が `#[cfg(debug_assertions)]` と `#[cfg(not(debug_assertions))]` で完全に別実装になっている。`debug_assert!` は release ビルドで自動消去されるため、分岐自体が不要であり、片方の修正が他方に反映されないリスクがある。

## 優先度根拠

- 約 15 行の完全なコピペで保守性リスク
- release ビルドで全バリデーションが省略され `status_class()` が panic し得る防御不足

## 現状

`src/decoder/head.rs:273-317` (RequestHead):
```rust
#[cfg(debug_assertions)]
pub(crate) fn from_validated_parts(...) -> Self {
    debug_assert!(is_valid_method(&method), ...);
    ...
    Self { method, uri, version, headers }
}

#[cfg(not(debug_assertions))]
pub(crate) fn from_validated_parts(...) -> Self {
    Self { method, uri, version, headers }
}
```

`ResponseHead` も `head.rs:462-509` で同様の重複。

## 設計方針

`debug_assert!` は release ビルドで自動消去されるため、`#[cfg(debug_assertions)]` 版のみに統合する。`#[cfg(not(debug_assertions))]` 版は削除する。

## 完了条件

- `from_validated_parts` が各型で 1 つだけ存在すること
- `debug_assert!` が release ビルドでコンパイル時に除去されること
- `cargo test` で全テストが通過すること
