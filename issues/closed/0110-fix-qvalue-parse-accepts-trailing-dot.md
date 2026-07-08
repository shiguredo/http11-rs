# QValue::parse が RFC 9110 Section 12.4.2 に違反して "0." と "1." を不正に受理するバグを修正する

- Priority: High
- Completed: 2026-07-08
- Model: DeepSeek V4 Pro

## 目的

`QValue::parse` が RFC 9110 Section 12.4.2 の ABNF に違反して、`"0."` と `"1."` を不正に受理しているバグを修正する。

ABNF:
```
qvalue = ( "0" [ "." 1*3DIGIT ] ) / ( "1" [ "." 1*3"0" ] )
```

`[ ... ]` は optional だが、`"."` が存在する場合は後続の桁が必須。`"0."` は `1*3DIGIT` を 0 桁で満たさず、`"1."` は `1*3"0"` を 0 個で満たさない。

## 優先度根拠

パース結果が仕様と異なる実際のバグであり、Accept ヘッダーの q 値重み付け判定に影響する。High。

## 現状

`src/accept.rs:94-117` で `QValue::parse` を実装。

- **"1." の経路** (L94-96): `rest.is_empty()` のときに明示的に `Ok(QValue(1000))` を返している
- **"0." の経路** (L108-117): `rest` が空の場合、`rest.len() > 3` が `false`、`!rest.chars().all(|c| c.is_ascii_digit())` が空真理で `false` となり、チェックをすり抜けて `QValue(0)` を返す

テスト `tests/test_accept.rs:75` は誤って `"1."` の受理を期待している:
```rust
assert_eq!(QValue::parse("1.").unwrap().value(), 1000);
```

## 設計方針

RFC 9110 Section 12.4.2 の ABNF に厳密に従い、`rest.is_empty()` の場合は `Err(AcceptError::InvalidQValue)` を返す。

1. `"1."` の経路 (L95-96): `return Ok(QValue(1000))` → `return Err(AcceptError::InvalidQValue)`
2. `"0."` の経路 (L108-109): `rest.is_empty()` を先頭でチェックし `Err(AcceptError::InvalidQValue)` を返す
3. テスト `test_qvalue_one_variants` の `"1."` 行を `is_err()` に修正
4. `"0."` の拒否を検証するテストケースを追加

## 完了条件

- `QValue::parse("0.")` が `Err(AcceptError::InvalidQValue)` を返す
- `QValue::parse("1.")` が `Err(AcceptError::InvalidQValue)` を返す
- `QValue::parse("1.0")`, `QValue::parse("0.5")` 等の正しい入力は引き続き受理される
- 既存テストが全て通過し、修正後のテストが追加されている

## 解決方法

- `src/accept.rs:95-96`: `rest.is_empty()` 時に `Ok(QValue(1000))` → `Err(AcceptError::InvalidQValue)`
- `src/accept.rs:109-110`: `"0."` 経路に `rest.is_empty()` の早期チェックを追加
- `tests/test_accept.rs:48-50`: `"0."` と `"1."` の拒否テストを追加
- `tests/test_accept.rs:78`: `"1."` の期待値を `is_err()` に修正
