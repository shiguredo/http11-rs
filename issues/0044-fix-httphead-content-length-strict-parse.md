# 0044: HttpHead::content_length を decoder/body と整合した厳格パースに統一する

Created: 2026-05-12
Model: Opus 4.7

## 概要

`HttpHead::content_length` は `Content-Length` ヘッダーの最初の値に `u64::from_str` を直接適用するだけで、decoder/body 側の厳格パース (`parse_content_length` / `parse_content_length_value`) と挙動が乖離している。

```rust
// src/decoder/head.rs:117-120
fn content_length(&self) -> Option<u64> {
    self.get_header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok())
}
```

一方、decoder body 側は OWS trim、カンマ区切りリストの同値マージ、`is_ascii_digit` 限定、複数行 `Content-Length` の同値検証など RFC 9110 §8.6 / §5.6.3 / §5.6.1.2 に準拠した厳格な解釈をしている (`src/decoder/body.rs:1354-1408`)。

## 根拠

### 差分検証

| 入力 (`Content-Length`) | `head.content_length()` | body の `parse_content_length_value` |
|---|---|---|
| `"100"` | `Some(100)` | `Ok(100)` |
| `"+100"` | **`Some(100)`** (`u64::from_str` は `+` を受理) | **`Err`** (`+` は ascii_digit でない) |
| `"0100"` | `Some(100)` | `Ok(100)` |
| `" 100 "` (ASCII OWS) | **`None`** | **`Ok(100)`** (`trim_ows` で除去) |
| `"100, 100"` | **`None`** (`,` は数字でない) | **`Ok(100)`** (カンマ split + 同値マージ) |
| `"100, 101"` | **`None`** | **`Err`** (mismatched values, smuggling 検知) |
| 複数行 `Content-Length: 100` + `Content-Length: 101` | **`Some(100)`** (`get_header` は最初の 1 件) | **`Err`** (smuggling 検知) |

### HTTP Request Smuggling (CWE-444) 経路

特に致命的なのは「decoder 本体は smuggling 検知 (mismatched values) で接続を切るのに、`HttpHead::content_length()` は最初の値を黙って返す」構造。reverse proxy 等の中継経路で `head.content_length()` を信用して下流に CL ヘッダーを再生成すると、本来 reject されるべき smuggling が trait 越しに完全バイパスされる。

### 実利用箇所

`examples/http11_reverse_proxy/src/main.rs:559-563` で HEAD レスポンスの CL 転送に `resp_head.content_length()` を使用:

```rust
let content_length = match body_kind {
    BodyKind::ContentLength(len) => Some(len),
    BodyKind::None if is_head => resp_head.content_length(),
    _ => None,
};
```

### RFC

- RFC 9110 §8.6: `Content-Length = 1*DIGIT`。複数ヘッダーで同値はマージ可能、異値は reject
- RFC 9110 §5.6.3: OWS = `*( SP / HTAB )` (Unicode 空白は対象外)
- RFC 9110 §5.6.1.2: empty list elements MUST be ignored

## 影響範囲

- 公開 trait `HttpHead` 経由で decoder の smuggling 検知をバイパス可能
- reverse proxy / 中継アプリで `head.content_length()` を信用すると HRS 経路を生む
- `Request::new(...).header("Content-Length", " 100 ")` のような構築経路と decoder 経路で挙動が異なる

## 対応方針

### `src/decoder/head.rs::HttpHead::content_length`

- `parse_content_length(self.headers())` 相当の実装に置き換える
- `pub(crate)` で `parse_content_length` を expose し、trait 実装側でも再利用する
- 戻り型を `Result<Option<u64>, Error>` に変えて smuggling 検知時はエラーを返す破壊的変更も検討する (canary 期間中なので許容)

### テスト

- `tests/test_decoder.rs` に上記差分検証表のケースを `HttpHead::content_length` 経由で再現する単体テストを追加
- PBT で「同じヘッダー集合に対して decoder の `BodyKind` と `head.content_length()` が整合する」性質を検証

### CHANGES.md

`## develop` に `[FIX]` として追加する (戻り型変更を伴う場合は `[CHANGE]`)。
