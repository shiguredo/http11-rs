# ContentRange::length() の整数オーバーフローを修正し、new_bytes() にバリデーションを追加する

- Priority: High
- Created: 2026-05-15
- Completed: 2026-05-15
- Model: deepseek v4-pro
- Branch: feature/fix-content-range-overflow

## 目的

`ContentRange::length()` (`src/range.rs:384`) が `e - s + 1` を unchecked に計算しており、2 つの問題がある:

1. `e = u64::MAX, s = 0` のとき debug ビルドで panic 、 release ビルドで wrapping （0 になる）
2. `new_bytes()` が `start > end` を検証しないため、`s=u64::MAX, e=0` のような不正な入力でも wrapping が発生する（`0 - u64::MAX + 1` → wrapping で `2`）

いずれも `checked_sub` + `checked_add` で統一的に解決する。

また `new_bytes()` (`src/range.rs:342-349`) が `start > end` と `complete_length <= end` の両方を検証していない。`new_bytes()` は `pub fn` であり、RFC 9110 Section 14.4 の validity rule（後述）に違反する Content-Range を構築可能な状態になっている。

本 issue のスコープは `ContentRange` に限定する。`RangeSpec` 側の `to_bounds()` や `parse_range_spec()` の overflow 関連経路は対象外とする。

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

これらの検証は RFC 9110 Section 14.4 の以下の validity rule に根拠を持つ（行 6634-6638）:

> A Content-Range field value is invalid if it contains a range-resp
> that has a last-pos value less than its first-pos value, or a
> complete-length value less than or equal to its last-pos value.
> The recipient of an invalid Content-Range MUST NOT attempt to
> recombine the received content with a stored representation.

- `last-pos < first-pos` → 実装: `start > end`
- `complete-length <= last-pos` → 実装: `len <= end`

## 設計方針

### 1. `length()` の安全化

`e.checked_sub(s).and_then(|d| d.checked_add(1))` に変更する。オーバーフロー時は `None` を返す。

**後方互換**: `length()` の戻り型は `Option<u64>` のまま。既存の `Some(n)` を返していた全パスは引き続き `Some` を返す。`(0, u64::MAX)` でのみ新たに `None` が返る。

### 2. `new_bytes()` に `start > end` と `complete_length <= end` の検証を追加する

`new_bytes()` は `pub fn` であり外部 crate から直接呼ばれる API であるため、release ビルドでも有効な検証が必要。検証方式は `assert!` を採用する:

- `new_bytes()` はプログラム内部で静的に決定される値を Content-Range に構築する経路であり、`start > end` や `complete_length <= end` の入力は呼び出し元のロジック誤り（プログラミングエラー）である
- 対して `parse()` はネットワークからの外部入力を扱う経路であり、不正入力は `Err` として呼び出し元に回復可能な形で返す

検証ロジック自体は `parse()` 内で既に重複しているため、`fn validate_content_range_parts(start: u64, end: u64, complete_length: Option<u64>) -> Result<(), RangeError>` として抽出し、`parse()` から呼び出す（`src/range.rs` 内の private 関数）。`new_bytes()` 側は個別の `assert!` で実装する（panic メッセージに違反条件を明示するため）:

```rust
pub fn new_bytes(start: u64, end: u64, complete_length: Option<u64>) -> Self {
    assert!(start <= end, "ContentRange: start must be <= end");
    if let Some(len) = complete_length {
        assert!(len > end, "ContentRange: complete_length must be > last-pos");
    }
    ContentRange { ... }
}
```

注意: `end = u64::MAX` のときは `u64` の最大値が `end` を超えられないため `complete_length = Some(_)` を指定できない。この制約は `new_bytes()` の doc コメントに明記する。同様に `start > end` と `complete_length <= end` が panic を引き起こすことも doc に記載する。

**後方互換**: `start > end` または `complete_length <= end` を渡していた呼び出し元は新たに panic する。いずれも RFC 上 invalid な Content-Range を構築していたバグであり、`[FIX]` として CHANGES.md に記載する。

### 3. PBT strategy の拡張

`pbt/tests/prop_range.rs` の全 `ContentRange` 関連 strategy の `start` / `end` 範囲を `0u64..10000` (または `0u64..1000`) から `0u64..=u64::MAX` に拡張する。

対象テストと修正内容:

**`prop_content_range_length`** (line 260-271):
- `start` / `end` を `0u64..=u64::MAX` に変更する
- swap パターンで `start <= end` を保証する（既存の `let (start, end) = if ...` を維持）
- `Some(end + 100)` (line 265) → `end.checked_add(100)` に変更。`checked_add` が `None` のときは `complete_length = None` で `new_bytes()` を呼ぶ
- 既存の `prop_assert!(length.is_some())` (line 268) を削除し、以下に置き換える:
  ```rust
  let expected = end.checked_sub(start).and_then(|d| d.checked_add(1));
  prop_assert_eq!(cr.length(), expected);
  ```

**`prop_content_range_roundtrip`** (line 173-191):
- `start` / `end` を `0u64..=u64::MAX` に変更する。`total` の strategy は `1u64..=20000` のまま維持
- swap パターンで `start <= end` を保証する
- `total.max(end + 1)` (line 181) → `end.checked_add(1).and_then(|min| Some(total.max(min)))` に変更。`end = u64::MAX` のとき式が `None` になるため `complete_length = None` で `new_bytes()` を呼ぶ

**`prop_content_range_unknown_length`** (line 316-328):
- `start` / `end` を `0u64..=u64::MAX` に変更する
- このテストは `parse()` 経路のみで `new_bytes()` を呼ばないため、assert の影響は受けない。swap パターンで `start <= end` を保証する

**`prop_content_range_is_unsatisfied`** (line 274-285): `new_bytes(0, 99, Some(total))` の固定値を使用しており strategy 修正は不要。範囲拡張の影響を受けない。

### 4. fuzz target の拡充

`fuzz/fuzz_targets/fuzz_range.rs` に `ContentRange::new_bytes()` の直接構築経路を追加する。既存の UTF-8 文字列経路（`parse()` 経由）に加え、バイナリデータから `(start, end, complete_length)` を構成する分岐を追加する:

- `data.len() >= 24` → start / end / complete_length を各 8 バイト little-endian で構成
- `16 <= data.len() < 24` → start / end のみ、`complete_length = None`
- 16 バイト未満 → early return（何もしない）

構成後、`start <= end && cl.map_or(true, |cl| cl > end)` の事前検証を挟み、assert を回避する。assert 経路のテストは単体テストの `#[should_panic]` でカバーする。

### 5. 単体テストの追加

`tests/test_range.rs` に以下を追加する:

`length()` の境界値テスト:
- `(s=0, e=u64::MAX)` → `None`（唯一の overflow ケース）
- `(s=1, e=u64::MAX)` → `Some(u64::MAX)`（`s >= 1` では overflow しないことの確認）
- `(s=0, e=u64::MAX-1)` → `Some(u64::MAX)`（結果が u64 に収まる最大ケース）
- `(s=0, e=0)` → `Some(1)`
- `(s=u64::MAX, e=u64::MAX)` → `Some(1)`
- `(s=u64::MAX-1, e=u64::MAX)` → `Some(2)`（通常範囲の確認）

`new_bytes()` のバリデーションテスト:
- `#[should_panic(expected = "start must be <= end")]` — `new_bytes(10, 5, None)`
- `#[should_panic(expected = "complete_length must be > last-pos")]` — `new_bytes(0, 100, Some(50))`
- `#[should_panic(expected = "complete_length must be > last-pos")]` — `new_bytes(u64::MAX, u64::MAX, Some(u64::MAX))`
- `#[should_panic(expected = "complete_length must be > last-pos")]` — `new_bytes(0, u64::MAX, Some(0))`

`new_bytes()` の正常系テスト:
- `new_bytes(0, u64::MAX, None)` が正常終了し、`length()` が `None`、`Display` が `"bytes 0-18446744073709551615/*"` を返すこと

`parse()` 側の既存テスト修正:
- `test_content_range_parse_errors` に `complete_length <= end` のケース（`"bytes 0-100/50"`、`"bytes 0-0/0"`）を追加する。`"bytes 500-100/1000"` は `start > end` の既存ケース

`InvalidBounds` Display 変更:
- `src/range.rs:56` の Display を `"invalid range bounds (start > end)"` から `"invalid range bounds"` に変更する。`InvalidBounds` は `ContentRange::parse()` の `start > end` / `complete_length <= end` 両方と、`parse_range_spec()` の `start > end` で使われているため、簡略化により全経路のメッセージが統一される
- 併せて `tests/test_range.rs` の `test_range_error_display` の期待値 `"invalid range bounds (start > end)"` も `"invalid range bounds"` に更新する

## 完了条件

- `length()` が任意の `(start, end)` で panic も wrapping も発生しないこと（`checked_sub` + `checked_add` で安全化）
- `length()` が `(s=0, e=u64::MAX)` で `None` を返すこと
- `length()` が `(s=1, e=u64::MAX)` で `Some(u64::MAX)` を返すこと
- `new_bytes(start, end, _)` が `start > end` のとき `assert!` で panic すること（メッセージ: `"ContentRange: start must be <= end"`）
- `new_bytes(start, end, Some(cl))` が `cl <= end` のとき `assert!` で panic すること（メッセージ: `"ContentRange: complete_length must be > last-pos"`）
- `new_bytes()` の doc コメントに panic 条件が明記されていること
- PBT 全 `ContentRange` 関連テストが `u64::MAX` 境界値を含み、全テストが通過すること
- fuzz target (`fuzz_range.rs`) が `new_bytes()` 直接構築経路をカバーし `cargo fuzz run fuzz_range` が crash を報告しないこと
- 単体テスト (`tests/test_range.rs`) に上記の境界値テスト、`#[should_panic]` テスト、正常系テスト、`complete_length <= end` の parse エラーケースが追加されていること
- `InvalidBounds` の Display メッセージが `"invalid range bounds"` に変更され、`test_range_error_display` の期待値も更新されていること
- `parse()` の検証ロジックが `validate_content_range_parts()` として抽出され、重複が除去されていること
- `CHANGES.md` の `## develop` に `[FIX]` エントリが追加されていること
- 全既存テストが引き続き通過すること

## 解決方法

### `src/range.rs`

- `length()` の計算を `e - s + 1` から `e.checked_sub(s).and_then(|d| d.checked_add(1))` に変更し、オーバーフロー時は `None` を返すようにした
- `new_bytes()` に `assert!(start <= end)` と `assert!(len > end)` のバリデーションを追加した
- `new_bytes()` の doc コメントに panic 条件と `end = u64::MAX` 時の `complete_length` 制約を明記した
- `parse()` 内の `start > end` / `complete_length <= end` 検証を `validate_content_range_parts()` として抽出し、`parse()` から呼び出すようにした
- `InvalidBounds` の Display メッセージを `"invalid range bounds"` に簡略化した

### `tests/test_range.rs`

- `length()` の境界値テスト 6 件を追加した (`(0, u64::MAX)` → None、`(1, u64::MAX)` → Some(u64::MAX) 等)
- `new_bytes()` の `#[should_panic]` テスト 4 件を追加した (start > end、complete_length <= end)
- `new_bytes(0, u64::MAX, None)` の正常系テスト 1 件を追加した
- `test_content_range_parse_errors` に `complete_length <= end` のケース 2 件を追加した
- `test_range_error_display` の `InvalidBounds` 期待値を更新した

### `pbt/tests/prop_range.rs`

- `prop_content_range_length` / `prop_content_range_roundtrip` / `prop_content_range_unknown_length` の strategy を `0u64..=u64::MAX` に拡張した
- `end = u64::MAX` のとき `complete_length = None` にするよう `checked_add` で安全にガードした

### `fuzz/fuzz_targets/fuzz_range.rs`

- `ContentRange::new_bytes()` の直接構築経路を追加した (バイナリデータから start/end/complete_length を LE で構成し、事前検証後に構築・アクセサ・Display ラウンドトリップを検証)
