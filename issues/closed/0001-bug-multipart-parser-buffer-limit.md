# 0001: MultipartParser のバッファ上限がない

## 概要

`MultipartParser::feed()` が無条件に `extend_from_slice` するため、終端境界を送らない悪意ある入力に対してメモリが無制限に蓄積する。

`DecoderLimits` はデコーダー側にのみ適用され、`MultipartParser` には制限機構がない。

## 再現方法

```rust
let mut parser = MultipartParser::new("boundary");
loop {
    // 境界なしのデータを永続的に feed し続けると OOM になる
    parser.feed(&vec![b'a'; 1024 * 1024]);
}
```

## 影響箇所

- `src/multipart.rs:253` — `feed()` の `extend_from_slice`
- `src/multipart.rs:299` — `Initial` 状態で `Incomplete` を返す経路（バッファは解放されない）
- `src/multipart.rs:378` — `InPart` 状態で `Incomplete` を返す経路（同上）

## 関連: to_vec() コピーの増幅

多数のパートや大きなバッファに対して、境界発見のたびにバッファ残余部分を `to_vec()` でコピーしている。

- `src/multipart.rs:279`, `288`, `291`, `363`, `365`, `368`

バッファ上限がない状態では、このコピーコストも攻撃者が増幅させられる。バッファ上限を設けることで、コピー量の上限も間接的に制限できる。

## 対応方針

`MultipartParser` に `max_buffer_size: usize` フィールドを追加し、`feed()` または `next_part()` 内でチェックして `MultipartError` を返す。

Sans I/O の原則として呼び出し側責務でもあるが、`DecoderLimits` と同様に上限を持つ設計が望ましい。

## 解決方法

- `MultipartError::BufferOverflow { size, limit }` バリアントを追加
- `MultipartParser` に `max_buffer_size: usize` フィールドを追加 (デフォルト: 10MB)
- `MultipartParser::with_max_buffer_size(n)` ビルダーメソッドを追加
- `feed()` の戻り値を `Result<(), MultipartError>` に変更し、上限超過時に `BufferOverflow` を返す
- `tests/test_multipart.rs` に `test_multipart_parser_buffer_overflow` / `test_multipart_parser_buffer_within_limit` テストを追加
- 全テストファイルの `parser.feed(...)` 呼び出しを `parser.feed(...).unwrap()` に修正
