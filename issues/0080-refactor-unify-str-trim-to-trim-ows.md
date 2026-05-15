# ヘッダーパース系モジュールの str::trim() を trim_ows() に統一する

- Priority: Medium
- Created: 2026-05-15
- Model: deepseek v4-pro

## 目的

encoder/decoder 側で HTTP Request Smuggling (CWE-444) 対策として `trim_ows()` (ASCII SP/HTAB のみ除去) に統一しているが、ヘッダーパース系モジュールでは Rust 標準の `str::trim()` (Unicode 空白も除去) を使用している。前段プロキシとの解釈不一致で smuggling の足場になり得る。

## 優先度根拠

- 防衛層の一貫性として統一すべきだが、現状での実害は確認されていない
- すでに encoder/decoder 側で対策済みであり、残るのは「他のモジュールとの整合性」レベルの問題

## 現状

以下のファイルで `str::trim()` が使用されている:

- `src/host.rs:58`
- `src/cookie.rs:90, 98`
- `src/accept.rs:167`
- `src/trailer.rs:66`
- `src/upgrade.rs`
- `src/vary.rs`
- `src/body.rs:763`

## 設計方針

全箇所の `str::trim()` を `trim_ows()` に置換する。Host ヘッダーも対象に含める。RFC 9110 Section 7.2 の `Host = uri-host [ ":" port ]` の ABNF は OWS を含まないが、受信側の寛容性として OWS 除去は許容範囲であり、`trim_ows()` により ASCII OWS (SP/HTAB) のみを除去することで解釈の一貫性を保つ。

## テスト

置換対象の各モジュールの既存テストが引き続き通過することを確認する。新たなテスト追加は不要（trim の挙動変更は各モジュールのテストでカバーされる）。

## 完了条件

- src/ 以下の全 .rs ファイルで `str::trim()` が `trim_ows()` に置換されていること
- ただし `is_valid_field_value` 等の `trim()` が意図的に Unicode 空白を含む箇所は対象外
- `cargo test` で全テストが通過すること
- `CHANGES.md` の `## develop` の `### misc` に `[UPDATE]` エントリが追加されていること
