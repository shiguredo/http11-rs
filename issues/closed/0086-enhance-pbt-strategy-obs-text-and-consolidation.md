# PBT の header_value strategy に obs-text を追加し、strategy 重複を pbt/src/lib.rs に集約する

- Priority: High
- Created: 2026-05-15
- Model: deepseek v4-pro

## 目的

全 PBT ファイルの `header_value_char()` strategy が VCHAR + SP + HTAB のみに制限され、obs-text (0x80-FF / U+0080..=U+10FFFF) が一切含まれていない。実装側 (`is_valid_field_value`) が obs-text を受理している以上、PBT でカバーされていないのはテスト戦略上の欠陥である。

また strategy 定義 (`http_method` / `header_value` / `status_code` 等) が各ファイルで別々に定義されており、RTSP 対応有無等で不整合がある。

## 優先度根拠

- RFC 9110 Section 5.5 の `field-vchar = VCHAR / obs-text` に反する PBT カバレッジ不足
- AGENTS.md で「obs-text は対象」と明示されているにも関わらず PBT が未対応
- strategy 重複は保守性に重大な影響

## 現状

- `pbt/tests/prop_request.rs:32-54` (`header_value_char()`)
- `pbt/tests/prop_response.rs:128-139` (`header_value_char()`)
- `pbt/tests/prop_encoder.rs:49-50` (`header_value()`)
- `pbt/tests/prop_cookie.rs:13,38` (cookie-octet も未カバー)

## 設計方針

1. `pbt/src/lib.rs` に `field_vchar()` strategy (VCHAR + SP + HTAB + obs-text) を新設する
2. 全 strategy 定義を `pbt/src/lib.rs` に集約し、各テストから `use pbt::*;` で参照する
3. RFC 6265 cookie-octet の全範囲をカバーする `cookie_octet()` strategy を追加する
4. `_dummy in 0u8..1` の PBT マクロ悪用テスト 2 件を `#[test]` 単体テストに書き換える

## 完了条件

- 全 PBT の header_value strategy が obs-text を含むこと
- strategy 定義が各テストファイルに重複せず `pbt/src/lib.rs` に集約されていること
- cookie の strategy が RFC 6265 cookie-octet の全文字をカバーしていること
- `cargo test -p pbt` で全 PBT が通過すること
- `CHANGES.md` の `## develop` の `### misc` に `[UPDATE]` エントリが追加されていること
