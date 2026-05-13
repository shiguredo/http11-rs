# 0060: `RequestEncoder` / `ResponseEncoder` (ストリーミング型) の panic 安全性を検証する fuzz target を追加する

Created: 2026-05-13
Model: Opus 4.7

## 概要

`src/encoder.rs` に存在する `RequestEncoder<C: Compressor = NoCompression>` / `ResponseEncoder<C: Compressor = NoCompression>` (公開ストリーミングエンコーダ) の `compress_body` / `finish` / `reset` 経路が、現在の `fuzz/fuzz_targets/` (34 target) のいずれからも到達されていない。

バッチ API である `encode_request` / `encode_response` / `encode_chunk` / `encode_chunks` / `encode_request_headers` / `encode_response_headers` は、`fuzz_encode_request` / `fuzz_encode_response` / `fuzz_decoder_chunked` で網羅されているが、ストリーミング API は呼び出し順序・出力バッファサイズ・`reset` を挟む再利用パターン等の状態空間を持つにもかかわらず fuzz target が無い。

`NoCompression` 経由でこれらの API を任意操作列で叩く fuzz target `fuzz_streaming_encoder` を追加し、攻撃者が制御し得る入力サイズ・出力バッファサイズ・操作順序に対して panic / abort / 不変条件破れが発生しないことを担保する。

## 根拠

### 未到達 API の確認

`grep -lE 'RequestEncoder|ResponseEncoder' fuzz/fuzz_targets/*.rs` の結果は空 (`pub use encoder::{RequestEncoder, ResponseEncoder, ...}` でエクスポートされているが現存 target はどれも参照していない)。

`fuzz_encode_request` / `fuzz_encode_response` はバッチ API (`encode_request(&Request) -> Vec<u8>`) のみを叩く target であり、`RequestEncoder::compress_body(input, output)` / `RequestEncoder::finish(output)` / `RequestEncoder::reset()` の経路には到達しない。

### 状態空間の特徴

`RequestEncoder` / `ResponseEncoder` のストリーミング API は内部に `Compressor` を保持し、`compress_body` → `finish` → `reset` の遷移を持つステートフル型である。`NoCompression::compress` は `finished == true` の状態で呼び出すと `CompressionError::AlreadyFinished` を返す等、状態に依存した分岐を持つ (`src/compression.rs:229-231`, `:249-252`)。

任意の `(input.len(), output.len())` の組み合わせ、任意の `compress_body` / `finish` / `reset` 呼び出し順序を fuzz で生成することで、`fuzz_decoder_mut_buf` と同様のアプローチで状態遷移の panic 安全性を検証できる。

### 検査すべき不変条件

`NoCompression` は `Compressor` trait の no-op 実装だが、ラップする `RequestEncoder` / `ResponseEncoder` を含めた以下の不変条件は任意 `Compressor` 実装でも維持されるべき API 契約である。

- `compress_body` / `finish` の戻り値が `Ok(_)` の場合、`consumed <= input.len()` かつ `produced <= output.len()`
- `compress_body` / `finish` がどの操作順序でも panic しない (`Err(_)` は許容)
- `reset` の前後で `compress_body` / `finish` が再度呼び出し可能 (panic しない)

## スコープ

### 対象

- `fuzz/fuzz_targets/fuzz_streaming_encoder.rs` を新規追加
- `fuzz/Cargo.toml` に `[[bin]] name = "fuzz_streaming_encoder"` エントリを追加
- `RequestEncoder::<NoCompression>::new()` / `ResponseEncoder::<NoCompression>::new()` を両方とも fuzz する
- `compress_body` / `finish` / `reset` を任意操作列で叩く
- 戻り値の `consumed` / `produced` 値が `input.len()` / `output.len()` を超えないことを assert する

### 対象外

- `NoCompression` 以外の `Compressor` 実装 (gzip 等は本リポジトリには無い)
- `Decompressor` 経路 (`ResponseDecoder::peek_body_decompressed` 等) の追加 fuzz — 既存 `fuzz_decoder_response` がカバー範囲
- ストリーミング API そのものの後方互換性変更
- `Request` / `Response` のヘッダ encode との結合 (バッチ API 側で既にカバー)

## 対応方針

### 入力モデル

`fuzz_decoder_mut_buf.rs` を踏襲し、任意操作列を `arbitrary` で生成する。

```rust
#[derive(Arbitrary, Debug)]
enum FuzzOp {
    /// compress_body(input, output) を呼ぶ
    /// input_len / output_len はそれぞれ 0..=4096 で clamp
    Compress { input: Vec<u8>, output_len: u16 },
    /// finish(output) を呼ぶ
    Finish { output_len: u16 },
    /// reset() を呼ぶ
    Reset,
}

#[derive(Arbitrary, Debug)]
struct FuzzInput {
    /// 操作列。長さは 0..=64 で clamp
    request_ops: Vec<FuzzOp>,
    response_ops: Vec<FuzzOp>,
}
```

### 検証ロジック

各 op を順番に実行し、戻り値が `Ok(status)` の場合は以下を assert する:

- `status.consumed() <= input.len()`
- `status.produced() <= output_len`
- `Compress` の場合: `status.is_complete()` でない (NoCompression の契約)
- `Finish` 直後の `Compress` 呼び出しが `Err(AlreadyFinished)` であること
- `Reset` 後は `Finish` / `Compress` が再び成功し得ること

`Err(_)` の場合は panic しないこと自体が目的なので無視する。

### OOM 回避

`output_len` は `u16` を `as usize` でそのまま使うことで上限 65535 に自然 clamp する (`fuzz_decoder_mut_buf` 同様、巨大な `Vec::with_capacity` を避ける)。

### ファイル配置

- `fuzz/fuzz_targets/fuzz_streaming_encoder.rs`
- `fuzz/Cargo.toml` の bin エントリは既存エントリ末尾に追記

### ブランチ

`feature/add-fuzz-streaming-encoder` (`feature/add-` prefix、後方互換あり、issue 番号を含まない)。

### CHANGES.md

`## develop` の `### misc` サブセクションに `[ADD]` として追加する:

```
- [ADD] `RequestEncoder` / `ResponseEncoder` (ストリーミング API) の panic 安全性を検証する `fuzz_streaming_encoder` fuzz target を追加する
  - 既存の `fuzz_encode_request` / `fuzz_encode_response` はバッチ API (`encode_request` / `encode_response`) のみを叩く target であり、`compress_body` / `finish` / `reset` のストリーミング経路は未到達だった
  - `NoCompression` を Compressor として任意操作列を流し込み、`consumed <= input.len()` / `produced <= output.len()` の不変条件と `AlreadyFinished` / `reset` 後再利用の状態遷移を検証する
  - @voluntas
```

## 受け入れ基準

- `fuzz/fuzz_targets/fuzz_streaming_encoder.rs` が追加されている
- `fuzz/Cargo.toml` に `fuzz_streaming_encoder` bin エントリが追加されている
- `cd fuzz && cargo +nightly fuzz build fuzz_streaming_encoder` が成功する
- `CHANGES.md` `## develop` `### misc` に `[ADD]` エントリが追加されている
- 既存 33 target のビルドに影響を与えない

## RFC 参照

本 issue は API の panic 安全性 fuzz であり RFC 文面そのものは参照しない。`NoCompression` の挙動契約は `src/compression.rs:209-263` (`Compressor for NoCompression` impl) に依拠する。
