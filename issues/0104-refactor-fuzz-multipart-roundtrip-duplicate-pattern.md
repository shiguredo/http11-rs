# fuzz_multipart_roundtrip.rs の重複パターンを削除する

- Priority: Medium
- Created: 2026-05-26
- Model: Opus 4.7
- Branch: feature/refactor-fuzz-multipart-roundtrip

## 目的

`fuzz/fuzz_targets/fuzz_multipart_roundtrip.rs` のパターン 1 とパターン 2 が完全に同一のコードになっている。コメントには「attacker controlled boundary 経路」と記載されているが、実際のコードは `MultipartBuilder::with_boundary(&boundary)` → `build_payload` → `MultipartParser::new(&boundary)` → `drive_parser` という同じ処理を繰り返しているだけであり、fuzzing サイクルを無駄に消費している。

同種の問題が `fuzz_multipart_boundary.rs` にも存在する (0105 で対応)。

## 優先度根拠

- fuzzing の効率を下げている (同一入力に対して同じ処理を 2 回実行する)
- コメントと実装が乖離しており、コードの意図が不明確になっている

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

## 設計方針

- パターン 2 のコードブロック全体 (コメント含む) を削除する
- 残るパターン 1 のコメントから「パターン 1:」の番号表記を外す (パターンが 1 つだけになるため)
- 現在の `MultipartBuilder` / `MultipartParser` には検証をスキップするコンストラクタが存在しないため、パターン 2 で意図されていた「attacker controlled boundary」経路は API 上実現不可能であり、削除しても fuzzing カバレッジは変わらない

## 完了条件

- `fuzz_multipart_roundtrip.rs` のパターン 2 のコードブロック全体が削除されている
- パターン 1 のコメントから番号表記が除去されている
- `cargo fuzz build fuzz_multipart_roundtrip` が成功する
