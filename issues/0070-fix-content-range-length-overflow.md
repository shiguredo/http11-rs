# ContentRange::length() の整数オーバーフローを修正し、new_bytes() にバリデーションを追加する

- Priority: High
- Created: 2026-05-15
- Model: deepseek v4-pro
- Branch: feature/fix-content-range-length-overflow

## 目的

`ContentRange::length()` (`src/range.rs:384`) が `e - s + 1` を unchecked に計算しており、2 つの問題がある:

1. `e = u64::MAX, s = 0` のとき debug ビルドで panic、release ビルドで wrapping (0 になる)
2. `new_bytes()` が `start > end` を検証しないため、`s=u64::MAX, e=0` のような不正な入力でも wrapping が発生する（`0 - u64::MAX + 1` → wrapping で `2`）

いずれも `checked_sub` + `checked_add` で統一的に解決する。

また `new_bytes()` (`src/range.rs:342-349`) が `start > end` と `complete_length <= end` の両方を検証していない。`parse()` (`src/range.rs:322-331`) は両方とも実行時エラー (`RangeError::InvalidBounds`) を返しているため、構築経路間で不変条件の一貫性が崩れている。

## 優先度根拠

- `ContentRange::new_bytes()` と `ContentRange::length()` は共に `pub` であり、外部利用者が直接呼び出せる API
- `ContentRange::length()` の panic は DoS 経路になり得る
- `parse()` 経路では `start > end` / `complete_length <= end` が検証されるため既存 fuzz では到達不能だが、直接構築経路は未防御

## 現状

`src/range.rs:382-387`:
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

`src/range.rs:322-331` (`parse()` 側の検証、`new_bytes()` には未実装):
```rust
if start > end {
    return Err(RangeError::InvalidBounds);
}
if let Some(len) = complete_length
    && len <= end
{
    return Err(RangeError::InvalidBounds);
}
```

## 設計方針

### 1. `length()` の安全化

`e.checked_sub(s).and_then(|d| d.checked_add(1))` に変更する。オーバーフロー時は `None` を返す。

これにより `length()` の API 契約が拡張される: 従来は「`start` と `end` が共に `Some` なら必ず `Some`」だったが、修正後は「`start` と `end` が共に `Some` でも計算不能なら `None`」となる。RFC 9110 Section 14.4 の Content-Range では `last-pos` が `u64` で表現されるため、`length = last-pos - first-pos + 1` が `u64` を超えるケースは実用上存在しないと考えられるが、防御として `None` を返す。

**後方互換**: `length()` の戻り型は `Option<u64>` のまま。既存の `Some(n)` を返していた全パスは引き続き `Some` を返す。`(0, u64::MAX)` のような極端な入力でのみ新たに `None` が返る。`CHANGES.md` には `[FIX]` として記載する。

### 2. `new_bytes()` に `start > end` と `complete_length <= end` の検証を追加する

`new_bytes()` は `pub fn` であり外部 crate から直接呼ばれる API であるため、release ビルドでも有効な検証が必要。`parse()` と同等の以下 2 つの検証を追加する:

1. `start > end` → `debug_assert!` ではなく常時 `assert!` で panic させる（契約違反はプログラミングエラーであるため）
2. `complete_length.is_some_and(|len| len <= end)` → 同様に `assert!` で panic

これにより `parse()` 経路と `new_bytes()` 経路で不変条件の一貫性が保たれる。

### 3. PBT strategy の拡張

`pbt/tests/prop_range.rs` の `prop_content_range_length` の strategy を `u64::MAX` 付近の境界値を含む範囲 (`0u64..=u64::MAX`) に拡張する。strategy は `start <= end` と `complete_length.map_or(true, |cl| cl > end)` の制約を `prop_filter` で適用し、`parse()` で reject されない有効な入力のみを生成する。

プロパティ: `new_bytes(s, e, cl)` → `to_string()` → `parse()` → `length()` のラウンドトリップが一致すること。**注意**: 既存の `prop_assert!(length.is_some())` のアサーションを修正する必要がある。修正後は `(0, u64::MAX)` で `length()` が `None` を返すため、`length()` の結果は `e.checked_sub(s).and_then(|d| d.checked_add(1))` と一致することを検証する形に変更する。

### 4. fuzz target の拡充

`fuzz/fuzz_targets/fuzz_range.rs` に `ContentRange::new_bytes()` の直接構築経路を追加する。`&[u8]` データから `(u64, u64, Option<u64>)` を構成する方法: 先頭 8 バイトを `start` (little-endian)、次の 8 バイトを `end`、残り 1 バイトの最上位 bit で `complete_length` の有無を決め、有効な場合は次の 8 バイトを値とする。

**注意**: `new_bytes()` は `start > end` または `complete_length <= end` で `assert!` により panic する。fuzz では `start <= end` かつ `complete_length.map_or(true, |cl| cl > end)` の事前検証を入れ、assert panic を crash として報告させないようにする。検証対象は `length()` の計算と `Display` / アクセサの呼び出しが panic しないこと。

### 5. 単体テストの追加

`tests/test_range.rs` に以下を追加する:
- `length()` の境界値テスト: `(s=0, e=u64::MAX)` → `None`、`(s=0, e=0)` → `Some(1)`、`(s=u64::MAX, e=u64::MAX)` → `Some(1)`
- `new_bytes()` のバリデーションテスト: `#[should_panic]` で `start > end` および `complete_length <= end` が panic すること

## 完了条件

- `length()` が任意の `(start, end)` で panic も wrapping も発生しないこと（`checked_sub` + `checked_add` で安全化）
- `length()` が `(s=0, e=u64::MAX)` で `None` を返すこと
- `new_bytes(start, end, _)` が `start > end` のとき常時 `assert!` で panic すること
- `new_bytes(start, end, Some(cl))` が `cl <= end` のとき常時 `assert!` で panic すること
- PBT (`prop_range.rs`) が `u64::MAX` 境界値を含むこと
- fuzz target が `new_bytes()` 直接構築経路をカバーし `cargo fuzz run fuzz_range` が crash を報告しないこと
- 単体テスト (`tests/test_range.rs`) に境界値テストと `#[should_panic]` テストが追加されていること
- `CHANGES.md` の `## develop` に `[FIX]` エントリが追加されていること
- `ContentRange::parse()` のラウンドトリップテストが引き続き通過すること
