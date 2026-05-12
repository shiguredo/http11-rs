# 0046: Transfer-Encoding を HTTP/1.1 完全一致以外で受理しないように厳格化する

Created: 2026-05-12
Model: Opus 4.7

## 概要

`RequestDecoder::determine_body_kind` / `ResponseDecoder::determine_body_kind` の TE 禁止ガードは `version == "HTTP/1.0"` の **完全一致のみ** で、HTTP/0.9 / HTTP/2.0 / HTTP/3.0 / RTSP/1.0 / FOO/1.0 などのバージョン文字列はすべて素通しで chunked として処理してしまう。

```rust
// src/decoder/request.rs:297-309
if transfer_encoding_chunked {
    if version == "HTTP/1.0" {
        return Err(Error::InvalidData(
            "Transfer-Encoding is not defined in HTTP/1.0".to_string(),
        ));
    }
    return Ok(BodyKind::Chunked);
}
```

```rust
// src/decoder/response.rs:374-391
if version == "HTTP/1.0"
    && self.headers.iter().any(|(name, _)| name.eq_ignore_ascii_case("Transfer-Encoding"))
{
    return Err(Error::InvalidData(
        "Transfer-Encoding is not defined in HTTP/1.0".to_string(),
    ));
}
```

`is_valid_protocol_version` (`src/validate.rs:85-125`) は token + DIGIT 構文だけ見るので `HTTP/0.9`, `HTTP/2.0`, `HTTP/3.0`, `RTSP/1.0`, `FOO/1.0` を **すべて通す**。RTSP も受信側として許容する方針があるため version 受理は緩いが、TE フレーミング解釈はそれと独立に厳格化する必要がある。

## 根拠

### RFC 引用

```
RFC 9112 §6.1
A client MUST NOT send a request containing Transfer-Encoding unless
it knows the server will handle HTTP/1.1 requests (or later minor
revisions); such knowledge might be in the form of specific user
configuration or by remembering the version of a prior received
response.  A server MUST NOT send a response containing Transfer-
Encoding unless the corresponding request indicates HTTP/1.1 (or
later minor revisions).
```

RFC 9112 §1 / §1.3 は本仕様 (TE / chunked / framing) を HTTP/1.1 のために定義しており、HTTP/0.9 は別レガシ系統 (Appendix C.1)、HTTP/2.0 / HTTP/3.0 は別 RFC (RFC 9113 / 9114) で binary framing を持つ。

### 攻撃シナリオ (HRS)

- 攻撃者が `GET /foo HTTP/0.9\r\nTransfer-Encoding: chunked\r\n\r\n<chunks>` を送信
- 前段プロキシ (古い実装) は `HTTP/0.9` を request-line のみのプロトコルとして扱う / `HTTP/1.0` に正規化する
- 本実装は chunked として読み始める
- 境界がずれて HTTP Request Smuggling が成立

### 関連

- 致命 5 (CONNECT 2xx の TE/CL) と独立だが、HRS の同根問題群

## 影響範囲

- HTTP Request Smuggling の足場
- `HTTP/0.9` / `HTTP/2.0` 等を擬装したリクエストで chunked フレーミングが動く
- 上位プロキシとの版番号解釈差で境界ずれが発生する

## 対応方針

### `src/decoder/request.rs` / `src/decoder/response.rs::determine_body_kind`

- TE ガードを `version != "HTTP/1.1"` の否定条件に厳格化する
- RTSP 等 (RFC 7826 / RFC 2326) は本ライブラリのスコープだが、TE は HTTP/1.1 限定なので RTSP に Transfer-Encoding が来た場合も error 化する

### `src/validate.rs::is_valid_protocol_version`

- 構文検査としては緩いまま残す (RTSP 等の受信を許容する方針)
- TE / chunked フレーミング解釈は version === "HTTP/1.1" の完全一致でのみ有効化する設計に切り分ける

### 関連設計

- `version` を `String` の素片で持つ現状から、`enum HttpVersion { Http10, Http11, Rtsp10, Rtsp20, Other(String) }` のような構造化型へ寄せる選択肢を併せて検討する (別 issue 化候補)

### テスト

- `tests/test_decoder.rs` に「HTTP/0.9 / HTTP/2.0 / RTSP/1.0 + Transfer-Encoding: chunked が reject される」単体テストを追加
- `pbt/tests/prop_decoder/request.rs` / `response.rs` に PBT を追加

### CHANGES.md

`## develop` に `[FIX]` として追加する。
