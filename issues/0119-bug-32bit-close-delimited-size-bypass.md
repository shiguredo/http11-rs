# 32 ビット環境で close-delimited ボディのサイズ制限がすり抜けられる

- Priority: Medium
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-32bit-close-delimited-size-bypass
- Polished: 2026-06-13

## 目的

`ResponseDecoder::decode()` の close-delimited 経路で、`decoded_body.len().checked_add(len)` が `usize` 同士の加算になっているため、32 ビット環境で `max_body_size` の制限をすり抜けうる問題を修正する。

## 優先度根拠

Medium とする。32 ビット環境で `max_body_size` が `u32::MAX` を超える設定の場合に限り発生する。現代のサーバー環境では 64 ビットが主流だが、32 ビットターゲットを想定する本ライブラリとしては修正すべき。

## 現状

`src/decoder/response.rs:807-824` で以下のように計算している。

```rust
let new_size =
    self.decoded_body
        .len()
        .checked_add(len)
        .ok_or(Error::BodyTooLarge { ... })?;
if (new_size as u64) > self.limits.max_body_size {
    return Err(Error::BodyTooLarge { ... });
}
```

`consume_body` 内では `body_consumed`（`u64`）を使って `max_body_size` を正しくチェックしているが、`decode()` 内のこの事前チェックは `decoded_body.len()` と `len` を `usize` で加算している。

32 ビット環境では `decoded_body.len()` と `len` が `u32::MAX` に制限されるため、`max_body_size` が `u32::MAX` を超える場合、加算がオーバーフローしない範囲で `extend_from_slice` が実行され、実際には制限を超えるメモリ確保が発生しうる。

## 設計方針

1. `decoded_body.len()` と `len` を `u64` にキャストしてから `checked_add` し、その結果を `max_body_size` と比較する。

## 完了条件

- 32 ビット環境でも `max_body_size` を超える close-delimited ボディが、メモリ確保前に `BodyTooLarge` で拒否されること。
- 64 ビット環境の既存挙動が変わらないこと。
- 該当箇所のユニットテストを追加すること。
- `CHANGES.md` に修正内容を記載すること。

## 解決方法

- `src/decoder/response.rs:807-824` のサイズ計算を `u64` ベースに修正する。具体的には `self.decoded_body.len() as u64` と `len as u64` を `checked_add` して `self.limits.max_body_size` と比較する。
- close-delimited レスポンスで `max_body_size` を超えた場合に `BodyTooLarge` になるテストを `tests/test_decoder/` 以下に追加する。32 ビットターゲットのテストは実行環境に依存するため、64 ビット環境で計算パスをカバーするテストとする。
