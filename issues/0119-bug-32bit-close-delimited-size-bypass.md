# 32 ビット環境で close-delimited ボディのサイズ制限がすり抜けられる

- Priority: Medium
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-32bit-close-delimited-size-bypass
- Polished: 2026-06-16

## 目的

`ResponseDecoder::decode()` の close-delimited 経路で、`decoded_body.len().checked_add(len)` が `usize` 同士の加算になっているため、32 ビット環境で `max_body_size` の制限をすり抜けうる問題を修正する。本件は closed/0097 (`BodyChunkedData { remaining }` / `body_consumed` / `max_body_size` を `u64` に統一) で行われた `u64` 統一の **取りこぼし** に該当し、本 issue でその漏れを補完する。

## 優先度根拠

Medium とする。32 ビット環境で `max_body_size` が `u32::MAX` (約 4 GiB) を超える設定の場合に限り発生する。本ライブラリは `no_std` / WASM32 / 組み込み等の 32 ビットターゲットを想定しており、`DecoderLimits::unlimited()` は `u64::MAX` を返すため、`max_body_size` が `u32::MAX` を超える設定は現実的に発生しうる。RFC 9112 §6.3 (close-delimited) / §7.1 (large numerals は recipient 側で有界処理する) に従い、受信側の有界保証として修正必要。

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

1. `decoded_body.len()` と `len` を `u64` にキャストしてから `checked_add` し、その結果を `max_body_size` (既に `u64`) と比較する。具体的な書き換え:

   ```rust
   let current = self.decoded_body.len() as u64;
   let incoming = len as u64;
   let new_size = current.checked_add(incoming).ok_or(Error::BodyTooLarge {
       size: u64::MAX,
       limit: self.limits.max_body_size,
   })?;
   if new_size > self.limits.max_body_size {
       return Err(Error::BodyTooLarge {
           size: new_size,
           limit: self.limits.max_body_size,
       });
   }
   ```

   `BodyTooLarge { size: u64::MAX, limit }` のパターンは closed/0097 で `body.rs:332-334` 等で確立済みの形と一致させる。

2. 事前チェックの意義: `extend_from_slice` (`response.rs:785`) の **直前** にチェックすることで、`Vec` 拡張による無駄な allocation を 1 ステップ早く弾く。`consume_body` 内の事後チェックでは手遅れになるケース (Vec の reallocation コストが既に発生) があるため、事前チェックは保持する。

3. `request.rs` 側は対象外: RFC 9112 §6.3 「Note」によりリクエストは close-delimited を使えない (リクエストは Content-Length / Transfer-Encoding の有無で決定し、close-delimited 経路には到達しない)。実際 `request.rs:751` は `BodyKind::CloseDelimited | BodyKind::None => {}` で空処理。本 issue は `response.rs` のみが対象。

## 完了条件

- 32 ビット環境でも `max_body_size` を超える close-delimited ボディが、メモリ確保前に `BodyTooLarge` で拒否されること。
- 64 ビット環境の既存挙動が変わらないこと。
- `request.rs` は対象外であることを設計判断として明示し、変更を加えないこと。
- 該当箇所のユニットテストを `tests/test_decoder/` 以下に追加すること:
  - **具体テストケース** (64 ビット環境でロジックパスを通すための代表ケース):
    - `max_body_size = 100`、`decoded_body` に 50 バイト蓄積済み、追加 `len = 60` (合計 110 > 100) で `BodyTooLarge` を返すこと (事前チェックが効くこと)
    - `max_body_size = 100`、`decoded_body` に 50 バイト蓄積済み、追加 `len = 50` (合計 100 ≤ 100) で受理されること (境界値: 等号は受理)
    - `max_body_size = u64::MAX`、`decoded_body` 0 バイト、追加 `len = usize::MAX` のようなオーバーフロー境界 (64 ビット環境では `usize::MAX = u64::MAX` のため `checked_add` 自体は overflow しないが、cast パスが正しく `u64` で行われることを確認)
  - **32 ビット環境専用テスト** (`#[cfg(target_pointer_width = "32")]`) は設けない。理由:
    - 本リポジトリの CI は主に 64 ビット環境 (`x86_64-unknown-linux-gnu` / `aarch64-apple-darwin`) で実行されており、32 ビットターゲットのクロスコンパイル + 実行環境は整備されていない
    - 32 ビット境界の実害再現 (`max_body_size > u32::MAX` + `decoded_body.len() + len` が 32 ビット範囲に収まり 64 ビット比較で reject されないケース) は cross-compile + qemu 等の追加環境を要し、本 issue のスコープを超える
    - 代わりに本 issue では (a) ロジックを `as u64` 統一する単純修正、(b) 64 ビット環境でのロジック検証、で実装の正しさを担保する。32 ビット境界の実機検証は将来 CI 拡張時に別途追加する
- `CHANGES.md` の `## develop` セクションに `[FIX]` エントリを `[ADD]` の下、`### misc` の上に追加すること。例: `[FIX] 32 ビット環境で close-delimited ボディのサイズ事前チェックが usize 加算で max_body_size をすり抜ける問題を修正する (closed/0097 の u64 統一の取りこぼし補完)`。

## 解決方法

1. `src/decoder/response.rs:807-824` のサイズ計算を設計方針 1 の Skeleton に従って `u64` ベースに修正する。
2. テストを `tests/test_decoder/` 以下に追加する (64 ビット環境でロジックを担保する形式)。
3. `CHANGES.md` に `[FIX]` エントリを追加する。

## 参考: RFC 文面

- RFC 9112 Section 6.3 Message Body Length (item 8)
  > If this is a response message and none of the above are true, then the message body length is determined by the number of octets received prior to the server closing the connection.
- RFC 9112 Section 6.3 Note (リクエスト側に close-delimited 経路が無い根拠)
  > Since there is no way to distinguish a successfully completed, close-delimited response message from a partially received message interrupted by network failure, a server SHOULD generate encoding or length-delimited responses whenever possible.
- RFC 9112 Section 7.1 (large numerals は recipient 側で有界処理)
  > A server that receives a request method that is unrecognized or not implemented needs to respond with the 501 (Not Implemented) status code. A server that receives a request body length larger than it is willing to accept ought to respond with a 4xx (Client Error) status code.

## 関連 issue

- closed/0097 `BodyChunkedData { remaining }` / `body_consumed` / `max_body_size` を `u64` に統一 (本 issue はその取りこぼし補完)
