# 0045: ResponseDecoder の CONNECT 2xx で Transfer-Encoding / Content-Length を error 化する

Created: 2026-05-12
Model: Opus 4.7

## 概要

`ResponseDecoder::determine_body_kind` は `request_method == "CONNECT"` かつ status code が 2xx の場合に **TE / CL の存在チェックなしで** 即 `BodyKind::Tunnel` を返す。

```rust
// src/decoder/response.rs:359-371
if self
    .request_method
    .as_deref()
    .is_some_and(|m| m == "CONNECT")
    && (200..300).contains(&status_code)
{
    return Ok(BodyKind::Tunnel);
}
```

RFC 9110 §9.3.6 は「A server MUST NOT send any Transfer-Encoding or Content-Length header fields in a 2xx (Successful) response to CONNECT」を MUST NOT として規定している。クライアント側は「MUST ignore」だが、`ResponseHead.headers` には CL / TE がそのまま残置されるため、上位アプリ (reverse proxy 等) が `HttpHead::content_length()` / `is_chunked()` を Tunnel 状態の head に対して呼ぶと CL / TE が観測される。

## 根拠

### 攻撃シナリオ (HRS)

- 攻撃者制御 origin が `HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\n` を返し、その直後にトンネル化したバイト列を流す
- 本実装 (`set_request_method("CONNECT")` 経由) は `BodyKind::Tunnel` を返して TE/CL をフレーミングとしては ignore する
- しかし `ResponseHead.headers` には `Content-Length: 5` が残るため、上位 proxy 実装がこの値を信用して下流に CL を再生成する経路で smuggling が成立する
- 致命 4 (`HttpHead::content_length` 不一致) と組み合わさることで実害が顕在化する

### RFC 引用

```
RFC 9110 §9.3.6
A server MUST NOT send any Transfer-Encoding or Content-Length header
fields in a 2xx (Successful) response to CONNECT. A client MUST
ignore any Content-Length or Transfer-Encoding header fields received
in a successful response to CONNECT.
```

「MUST ignore」を「ResponseHead から物理的に取り除く」と解釈するのが堅牢設計。MUST NOT 違反入力を error 化することは仕様準拠の範囲内 (受信側が SHOULD reject に倒すことは MUST ignore と両立する)。

## 影響範囲

- 上位アプリが CONNECT 2xx の `ResponseHead.headers` から TE/CL を読んで下流に再送する経路で HRS の足場になる
- 本ライブラリ内の `examples/http11_reverse_proxy` は `BodyKind::Tunnel` を別経路に分岐しているため直接の exploit は本リポジトリ内に観測できないが、利用者が proxy を書く際の落とし穴
- 致命 4 と組み合わせて修正することで HRS 経路を完全に塞げる

## 対応方針

### `src/decoder/response.rs::determine_body_kind`

- CONNECT + 2xx 分岐の前に「`Content-Length` / `Transfer-Encoding` のいずれかが存在したら `Error::InvalidData("CONNECT 2xx response must not contain Transfer-Encoding or Content-Length")` を返す」ガードを追加する
- 並行して `src/decoder/request.rs` の CONNECT リクエスト経路 (`request.rs:460-461` 周辺) でも CL/TE 受信を error 化することを対称的に検討する

### テスト

- `tests/test_decoder.rs` に CONNECT 2xx + CL / TE 受信時の reject 単体テストを追加
- `pbt/tests/prop_decoder/response.rs` に PBT を追加

### CHANGES.md

`## develop` に `[FIX]` として追加する。

### 関連 issue

- 0044 (HttpHead::content_length の厳格パース) と併せて HRS 経路を塞ぐ
