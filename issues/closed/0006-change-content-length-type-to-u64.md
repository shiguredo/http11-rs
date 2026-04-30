# 0006: Content-Length の型を u64 に変更し overflow を明示的に処理する

Created: 2026-04-28
Completed: 2026-04-30
Model: Kimi 2.6 / GPT 5.5 / Composer 2 Fast

## 概要

`Request::content_length()`、`Response::content_length()`、`HttpHead::content_length()`、および decoder 内部の `parse_content_length()` / `BodyKind::ContentLength` / `DecodePhase::BodyContentLength` の型を `usize` から `u64` に変更し、32bit 環境での overflow と precision loss を防ぐ。

## 根拠

RFC 9110 Section 8.6:

> Content-Length = 1*DIGIT
>
> Any Content-Length field value greater than or equal to zero is valid.
>
> Since there is no predefined limit to the length of content, a recipient MUST anticipate potentially large decimal numerals and prevent parsing errors due to integer conversion overflows or precision loss.

32bit 環境では `usize` が 4GB 付近で上限に達し、RFC が要求する「大きな 10 進数を overflow / precision loss なしに扱う」実装にならない。

## 対象ファイルと変更点

### 公開 API

- `src/request.rs` - `content_length() -> Option<u64>`
- `src/response.rs` - `content_length() -> Option<u64>`
- `src/decoder/head.rs` - `HttpHead::content_length() -> Option<u64>`
- `src/decoder/body.rs` - `BodyKind::ContentLength(usize)` → `BodyKind::ContentLength(u64)`
  - `BodyKind` は `lib.rs` で公開 re-export されている (`pub use decoder::{BodyKind, ...}`) ので、これも破壊的変更。

### 内部実装

- `src/decoder/body.rs`:
  - `parse_content_length() -> Result<Option<u64>, Error>`
  - `parse_content_length_value() -> Result<u64, Error>`
  - `resolve_body_headers_for_request()` / `resolve_body_headers_for_response()` の戻り値も `Option<u64>` に変更
- `src/decoder/phase.rs`:
  - `DecodePhase::BodyContentLength { remaining: usize }` → `DecodePhase::BodyContentLength { remaining: u64 }`
- `src/decoder/request.rs` / `src/decoder/response.rs`:
  - `determine_body_kind()` 内で `content_length` を `u64` として受け取る
  - `len > self.limits.max_body_size` の比較を安全に行う（`max_body_size` は `usize` のまま）
  - `available_body_len()` で `self.buf.len().min(*remaining)` を `u64` 対応に
  - `consume_body()` で `remaining` の減算を `u64` で行う

## 対応方針

### 1. パース部分

`parse_content_length_value()` で `part.parse::<u64>()` を使用。`u64` 範囲外の値は `Error::InvalidData("invalid Content-Length: overflow")` を返す（現状と同じエラー種別）。

### 2. BodyKind / DecodePhase

`BodyKind::ContentLength(u64)` と `DecodePhase::BodyContentLength { remaining: u64 }` に変更。chunked のチャンクサイズは既に `usize`（バッファサイズに直結）なのでそのまま。

### 3. サイズ制限との照合

`determine_body_kind()` 内で:

```rust
if let Some(len) = content_length {
    // max_body_size は usize、len は u64
    if len > self.limits.max_body_size as u64 {
        return Err(Error::BodyTooLarge {
            size: usize::try_from(len).unwrap_or(usize::MAX),
            limit: self.limits.max_body_size,
        });
    }
    return Ok(BodyKind::ContentLength(len));
}
```

`usize::try_from(len).unwrap_or(usize::MAX)` とすることで、`len` が `usize` の範囲内なら実サイズを報告し、`usize` を超える場合のみ `usize::MAX` にフォールバックする。これにより通常ケース（例: limit 10MB に対する Content-Length 10MB+1）では実サイズを失わない。`Error::BodyTooLarge` の `size` フィールドを `u64` に変更する案もあるが、他の `usize` ベースのサイズ制限（`max_buffer_size` 等）との整合性を考慮し、本 issue では `usize` のままとする。

### 4. バッファ長との比較

`available_body_len()` では `self.buf.len()`（usize）と `remaining`（u64）を比較する。

```rust
let available = if *remaining >= self.buf.len() as u64 {
    self.buf.len()
} else {
    *remaining as usize
};
```

### 5. エンコーダー側

`encoder.rs` の `validate_content_length_headers()` は既に `Option<u64>` を返しているため影響なし。`Request::content_length()` / `Response::content_length()` を呼び出している箇所は `u64` に対応する必要があるが、エンコーダー側では `as u64` キャストが不要になる。

## 影響範囲

- `Request::content_length()` / `Response::content_length()` / `HttpHead::content_length()` のシグネ変更は破壊的変更。
- `BodyKind::ContentLength` の型変更は decoder 出力の型変更となり、呼び出し側に影響。
- `Error::BodyTooLarge` の `size` フィールドは本 issue では `usize` のままとし、超過時は `usize::MAX` を固定して報告する。将来的に `u64` 化を検討する余地あり。

## 検証

- `make fmt && make clippy && make check && make test` を通す。
- 32bit ターゲットでのビルド確認（可能であれば）。
- `Content-Length` が `u64::MAX` を超えるような入力に対して、適切にエラーが返されることを確認する。
- `Content-Length: 4294967296`（4GB+1）のような値が 32bit `usize` では overflow せず `u64` で正しく処理されることを確認する。
- `CHANGES.md` に `[CHANGE]` セクションを追記する。

## 解決方法

- `HttpHead::content_length()` / `Request::content_length()` / `Response::content_length()` の戻り値を `Option<u64>` に変更した。
- `BodyKind::ContentLength` を `u64` に、`DecodePhase::BodyContentLength { remaining: u64 }` に変更した。
- `parse_content_length()` / `parse_content_length_value()` / `resolve_body_headers_for_*()` を `u64` に対応させた。
- `determine_body_kind()` で `max_body_size` との比較を `u64` 対応にし、`BodyTooLarge` 報告時は `usize::try_from(len).unwrap_or(usize::MAX)` でフォールバックした。
- `BodyDecoder::peek_body()` / `available_body_len()` で `usize` バッファ長と `u64` remaining の比較を安全に行うようにした。
- `BodyDecoder::consume_body()` で `remaining` の減算を `u64` で行うようにした。
- PBT テストと fuzz テスト、`skills` のドキュメントも `u64` に対応させた。
