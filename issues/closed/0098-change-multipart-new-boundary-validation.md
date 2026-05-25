# MultipartParser::new / MultipartBuilder::with_boundary の boundary 検証を追加する

- Priority: Medium
- Created: 2026-05-25
- Completed: 2026-05-25
- Model: Opus 4.7
- Branch: feature/change-multipart-boundary-validation

## 目的

`MultipartParser::new` が boundary の検証を一切行わずに任意の文字列を受け入れる。空の boundary を渡した場合、`first_delimiter` が `b"--"` のみになり、ボディ内の `--` がすべて boundary として誤検出される。

`MultipartBuilder::with_boundary` も同様に検証なしで boundary を受け入れる。

現在 `try_new` / `try_with_boundary` が検証付き API として存在するが、無検証の `new` / `with_boundary` が `pub` で公開されたままであり、利用者が誤って使用するリスクがある。

## 優先度根拠

RFC 2046 Section 5.1.1 の boundary 構文制約 (1-70 文字、bchars のみ) に違反する入力を受け入れるため、パース結果の正しさが保証されない。特に空 boundary は全 `--` を boundary として検出するため、セキュリティ上の問題 (WAF 迂回等) に繋がる可能性がある。

## 現状

- `MultipartParser::new` (行 277-297): boundary 検証なし、infallible
- `MultipartParser::try_new` (行 308-313): `is_valid_boundary` で検証、`Result` 返却
- `MultipartBuilder::with_boundary` (行 605): 検証なし、infallible
- `MultipartBuilder::try_with_boundary` (行 615-619): 検証あり、`Result` 返却

## 設計方針

`new` を `Result` 返却に変更し、`try_new` を削除する。`Request::new` / `Response::new` が `Result` を返す本プロジェクトのパターンに揃える。

### MultipartParser

```rust
pub fn new(boundary: &str) -> Result<Self, MultipartError> {
    if !is_valid_boundary(boundary) {
        return Err(MultipartError::InvalidBoundary);
    }
    // ... 既存の構築ロジック
    Ok(MultipartParser { ... })
}
```

- `try_new` は削除する (旧 `new` の呼び出し元は `.unwrap()` / `?` を追加)
- `try_new` が使われている箇所は `new` に置き換える (シグネチャ同一)

### MultipartBuilder

```rust
pub fn with_boundary(boundary: &str) -> Result<Self, MultipartError> {
    if !is_valid_boundary(boundary) {
        return Err(MultipartError::InvalidBoundary);
    }
    // ... 既存の構築ロジック
    Ok(Self { ... })
}
```

- `try_with_boundary` は削除する
- `MultipartBuilder::new(random_value: u64)` は内部で生成する boundary が常に有効なため変更不要

### 破壊的変更の影響

- `MultipartParser::new` の戻り値が `Self` → `Result<Self, MultipartError>` に変更
- `MultipartParser::try_new` が削除される
- `MultipartBuilder::with_boundary` の戻り値が `Self` → `Result<Self, MultipartError>` に変更
- `MultipartBuilder::try_with_boundary` が削除される
- crate 内の呼び出し箇所 (テスト含む) は全てハードコードされた有効 boundary を渡しているため、`.unwrap()` を追加するだけで移行可能

### Fuzz への影響

以下の fuzz ターゲットが `new` に任意 boundary を渡しているため改修が必要:
- `fuzz/fuzz_targets/fuzz_multipart_boundary.rs`: `new` → `new` + `Err` 時 early return
- `fuzz/fuzz_targets/fuzz_multipart_roundtrip.rs`: 同様
- `fuzz/fuzz_targets/fuzz_multipart.rs`: ハードコード boundary のため影響なし (`.unwrap()` 追加のみ)

fuzz ターゲットは `new` が `Err` を返す場合に early return し、パニック安全性の検証対象から除外する。boundary の構文検証自体は `is_valid_boundary` の単体テストでカバー済み。

## テスト戦略

### 単体テスト

`tests/test_multipart.rs` に以下を追加:
- 空 boundary → `Err(MultipartError::InvalidBoundary)`
- 71 文字以上 → `Err(MultipartError::InvalidBoundary)`
- 禁止文字含む → `Err(MultipartError::InvalidBoundary)`
- 有効 boundary → `Ok(_)`
- `MultipartBuilder::with_boundary` でも同様

### PBT

既存の `pbt/tests/prop_multipart.rs` の Strategy が有効 boundary を生成していることを確認。`new` が `Result` になるため、Strategy 側で無効 boundary を生成しないか、テスト内で `unwrap()` するかを選択。

### Fuzzing

上記「Fuzz への影響」セクション参照。

## 完了条件

- `MultipartParser::new` が不正な boundary (空文字列、71 文字以上、禁止文字を含む) で `Err` を返すこと
- `MultipartParser::try_new` が削除されていること
- `MultipartBuilder::with_boundary` が不正な boundary で `Err` を返すこと
- `MultipartBuilder::try_with_boundary` が削除されていること
- 既存テスト (PBT / 単体テスト / fuzz) が全て通ること
- 不正 boundary のテストが追加されていること
- `CHANGES.md` に `[CHANGE]` として破壊的変更を記載すること

## 解決方法

- `MultipartParser::new` に `is_valid_boundary` 検証を追加し、戻り値を `Result<Self, MultipartError>` に変更した
- `MultipartParser::try_new` を削除した
- `MultipartBuilder::with_boundary` に `is_valid_boundary` 検証を追加し、戻り値を `Result<Self, MultipartError>` に変更した
- `MultipartBuilder::try_with_boundary` を削除した
- crate 内の全呼び出し箇所 (~75 箇所) に `.unwrap()` または `if let Ok(...)` パターンを追加した
- `tests/test_multipart.rs` に boundary 検証テスト 6 件を追加した
- fuzz ターゲットは `Err` 時 early return パターンに修正した
