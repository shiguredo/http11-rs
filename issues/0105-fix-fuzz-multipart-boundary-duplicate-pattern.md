# fuzz_multipart_boundary.rs の重複パターンを削除する

- Priority: Medium
- Created: 2026-05-26
- Model: Opus 4.7
- Branch: feature/fix-fuzz-multipart-boundary-duplicate

## 目的

`fuzz/fuzz_targets/fuzz_multipart_boundary.rs` のパターン 1 (69-73 行目) とパターン 2 (75-80 行目) が完全に同一のコードになっている。コメントには「任意 boundary の経路 (検証に引っかかる境界は早期リターン)」と記載されているが、パターン 1 も `MultipartParser::new(&boundary)` を通しており差別化されていない。fuzzing サイクルを無駄に消費している。

## 優先度根拠

- fuzzing の効率を下げている (同一入力に対して同じ処理を 2 回実行する)
- コメントと実装が乖離しており、コードの意図が不明確になっている
- パニック安全性への影響はないため致命的ではないが、fuzzing の品質に関わる

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

両者は完全に同一。

## 設計方針

- パターン 2 を削除する
- 0104 と同じ理由で、現在の API では差別化が困難なため単純な削除が妥当

## 完了条件

- `fuzz_multipart_boundary.rs` のパターン 2 (75-80 行目) が削除されている
- `cargo fuzz build fuzz_multipart_boundary` が成功する
