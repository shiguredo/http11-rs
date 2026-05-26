# fuzz_multipart_roundtrip.rs の重複パターンを削除する

- Priority: Medium
- Created: 2026-05-26
- Model: Opus 4.7
- Branch: feature/fix-fuzz-multipart-roundtrip-duplicate

## 目的

`fuzz/fuzz_targets/fuzz_multipart_roundtrip.rs` のパターン 1 (118-124 行目) とパターン 2 (126-133 行目) が完全に同一のコードになっている。コメントには「attacker controlled boundary 経路」と記載されているが、実際のコードは `MultipartBuilder::with_boundary(&boundary)` → `build_payload` → `MultipartParser::new(&boundary)` → `drive_parser` という同じ処理を繰り返しているだけであり、fuzzing サイクルを無駄に消費している。

## 優先度根拠

- fuzzing の効率を下げている (同一入力に対して同じ処理を 2 回実行する)
- コメントと実装が乖離しており、コードの意図が不明確になっている
- パニック安全性への影響はないため致命的ではないが、fuzzing の品質に関わる

## 現状

パターン 1:
```rust
// パターン 1: `with_boundary` を通った valid path
if let Ok(builder) = MultipartBuilder::with_boundary(&boundary) {
    let payload = build_payload(builder, &parts);
    if let Ok(mut parser) = MultipartParser::new(&boundary) {
        drive_parser(&mut parser, &payload, split_size);
    }
}
```

パターン 2:
```rust
// パターン 2: attacker controlled boundary 経路。build 側 / parse 側どちらでも
// パニックしないことを確認する。
if let Ok(builder) = MultipartBuilder::with_boundary(&boundary) {
    let payload = build_payload(builder, &parts);
    if let Ok(mut parser) = MultipartParser::new(&boundary) {
        drive_parser(&mut parser, &payload, split_size);
    }
}
```

両者は完全に同一。

## 設計方針

- パターン 2 を削除する
- パターン 2 の意図 (attacker controlled boundary) を実現するなら、`with_boundary` を通さずに直接バイト列を構築する等の差別化が必要だが、現在の `MultipartBuilder` / `MultipartParser` の API では `with_boundary` / `new` を通さざるを得ないため、単純な削除が妥当

## 完了条件

- `fuzz_multipart_roundtrip.rs` のパターン 2 (126-133 行目) が削除されている
- `cargo fuzz build fuzz_multipart_roundtrip` が成功する
