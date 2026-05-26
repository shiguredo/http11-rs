# fuzz_multipart_boundary.rs の重複パターンを削除する

- Priority: Medium
- Created: 2026-05-26
- Completed: 2026-05-26
- Model: Opus 4.7
- Branch: feature/refactor-fuzz-multipart-boundary

## 目的

`fuzz/fuzz_targets/fuzz_multipart_boundary.rs` のパターン 1 とパターン 2 が完全に同一のコードになっている。コメントには「任意 boundary の経路 (検証に引っかかる境界は早期リターン)」と記載されているが、パターン 1 も `MultipartParser::new(&boundary)` を通しており差別化されていない。fuzzing サイクルを無駄に消費している。

同種の問題が `fuzz_multipart_roundtrip.rs` にも存在する (0104 で対応)。

## 優先度根拠

- fuzzing の効率を下げている (同一入力に対して同じ処理を 2 回実行する)
- コメントと実装が乖離しており、コードの意図が不明確になっている

## 現状

パターン 1:
```rust
// パターン 1: `new` 経路 (RFC 2046 Section 5.1.1 検証あり)
if let Ok(mut parser) = MultipartParser::new(&boundary) {
    parser = parser.with_max_buffer_size(max_buffer_size);
    drive(&mut parser, &data, split_size);
}
```

パターン 2:
```rust
// パターン 2: 任意 boundary の経路 (検証に引っかかる境界は早期リターン)
if let Ok(mut parser) = MultipartParser::new(&boundary) {
    parser = parser.with_max_buffer_size(max_buffer_size);
    drive(&mut parser, &data, split_size);
}
```

## 設計方針

- パターン 2 のコードブロック全体 (コメント含む) を削除する
- 残るパターン 1 のコメントから「パターン 1:」の番号表記を外す (パターンが 1 つだけになるため)
- 現在の `MultipartParser` には検証をスキップするコンストラクタが存在しないため、パターン 2 で意図されていた「検証なし経路」は API 上実現不可能であり、削除しても fuzzing カバレッジは変わらない

## 完了条件

- `fuzz_multipart_boundary.rs` のパターン 2 のコードブロック全体が削除されている
- パターン 1 のコメントから番号表記が除去されている
- `cargo fuzz build fuzz_multipart_boundary` が成功する

## 解決方法

- `fuzz/fuzz_targets/fuzz_multipart_boundary.rs` のパターン 2 (75-79 行) のコードブロック全体 (コメント含む) を削除した
- パターン 1 のコメント `// パターン 1: \`new\` 経路 (RFC 2046 Section 5.1.1 検証あり)` から番号表記を除去し `// \`new\` 経路 (RFC 2046 Section 5.1.1 検証あり)` に変更した
- `cargo +nightly fuzz build fuzz_multipart_boundary` でビルド成功を確認した
