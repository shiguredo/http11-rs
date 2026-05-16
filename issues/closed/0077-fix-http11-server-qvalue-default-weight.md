# examples/http11_server の Accept-Encoding qvalue デフォルト値を RFC 9110 Section 12.4.2 に準拠させる

- Priority: High
- Created: 2026-05-15
- Completed: 2026-05-15
- Model: deepseek v4-pro
- Branch: feature/fix-server-qvalue

## 解決方法

`examples/http11_server/src/compressor.rs:17` と `examples/http11_server_io_uring/src/compressor.rs:17` の `unwrap_or(1.0)` を `unwrap_or(0.0)` に変更し、RFC 9110 Section 12.4.2 の「無効な quality 値のデフォルトは 0」に準拠させた。

## 完了条件

## 目的

`examples/http11_server/src/compressor.rs:17` と `examples/http11_server_io_uring/src/compressor.rs:17` で `Accept-Encoding` の quality 値の parse 失敗時に `unwrap_or(1.0)` で重み 1.0 にフォールバックしている。RFC 9110 Section 12.4.2 は無効な quality 値のデフォルトを 0 と規定している。

## 優先度根拠

- RFC 9110 MUST 要件に違反
- クライアントが `Accept-Encoding: zstd;q=INVALID` を送信した場合、サーバーが zstd を選択し、クライアントが展開不能なレスポンスを受け取る
- サンプルは「お手本」であるため RFC 準拠が必須 (AGENTS.md:203)

## 現状

`examples/http11_server/src/compressor.rs:17`:
```rust
let q: f32 = part[pos + 3..].trim().parse().unwrap_or(1.0);
```

`examples/http11_server_io_uring/src/compressor.rs:17` も同様。

## 設計方針

`unwrap_or(1.0)` を `unwrap_or(0.0)` に修正する。名前付き関数に切り出し、RFC 9110 Section 12.4.2 の参照コメントを添える。

## 完了条件

- `Accept-Encoding: zstd;q=badvalue` に対して zstd が選択されないこと
- `Accept-Encoding: gzip;q=0.8, zstd;q=INVALID, br;q=0.5` で zstd が選択されず gzip が選択されること
- `examples/http11_server` の既存テスト (`tests/http_basic.rs`) が引き続き通過すること
- CHANGES.md の `## develop` に `[FIX]` エントリが追加されていること
