# 0040: HttpHead::is_keep_alive を HTTP/1.1 完全一致に変更する

Created: 2026-05-12
Model: Opus 4.7

## 概要

`src/decoder/head.rs::HttpHead::is_keep_alive` は `Connection` ヘッダーを評価した後のフォールバックで `self.version().ends_with("/1.1")` を使う。

```rust
self.version().ends_with("/1.1")
```

これは以下の問題を持つ:

1. `RTSP/1.1` (将来) や独自プロトコル `FOO/1.1` のような version 文字列で true を返してしまう
2. RTSP (RFC 7826) は persistent connection が前提のプロトコルで HTTP/1.1 と意味論が異なる。本クレートのスコープには RTSP/1.0 / RTSP/2.0 が含まれるが、`is_keep_alive` は HTTP セマンティクスを返すメソッドであり RTSP の挙動と混同してはならない
3. 検査が「サフィックス一致」のため、`is_valid_protocol_version` が許容する任意の token プロトコル名 (例: `XYZ/1.1`) で誤動作する

対応として、HTTP/1.1 の persistent connection 判定は `version == "HTTP/1.1"` の完全一致に限定する。RTSP やその他プロトコルの persistent connection 判定は上位層の責務とする。

## 根拠

### RFC

- RFC 9112 Section 9.3: HTTP/1.1 message recipients は persistent connection を前提とする
- RFC 9112 Section 9.1 (HTTP-version の case-sensitive 検査)
- RFC 7826 (RTSP 2.0) は別プロトコルとして persistent connection を独自に定義

### 影響範囲

- 攻撃シナリオではないが、RTSP / 独自プロトコルを扱う proxy 等で意図しない keep-alive 動作を起こす
- 本クレートの doc コメントは「HTTP/1.1 のみ keep-alive にする」と読める方向性で書かれており、`ends_with("/1.1")` は実装の不備

## 対応方針

### `src/decoder/head.rs::HttpHead::is_keep_alive`

`self.version().ends_with("/1.1")` を `self.version() == "HTTP/1.1"` に変更する。

doc コメントの「version 文字列が `/1.1` で終わる場合のみ `true`」を「`version == "HTTP/1.1"` のときのみ `true`」に修正する。`Request` / `Response` / `RequestHead` / `ResponseHead` の同等メソッドの doc も追随する。

### テスト

- `tests/test_decoder.rs` or `tests/test_request.rs`: `RTSP/1.1` / `FOO/1.1` のような version で `is_keep_alive` が `false` になることを確認
- `HTTP/1.0` で `Connection: keep-alive` がない場合は引き続き `false`
- `HTTP/1.1` で `Connection: close` がない場合は引き続き `true`
- `HTTP/1.1` で `Connection: close` がある場合は引き続き `false`
- `pbt` 側で類似の strategy を持っている場合は調整

### CHANGES.md

`## develop` のメインに `[FIX]` として追記する。

### 破壊的変更

- 旧挙動 (RTSP/1.1 等で true を返していた) に依存していたユーザーは false を返すようになる
- canary リリース中なので破壊的変更は許容範囲
- HTTP/1.1 完全一致は本来の意図 (doc コメント) に沿った挙動
