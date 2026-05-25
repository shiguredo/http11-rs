# src/ 内のインラインテストを tests/test_<module>.rs に外部化する (第 2 弾)

- Priority: Medium
- Created: 2026-05-25
- Model: Opus 4.7
- Branch: feature/refactor-extract-inline-tests

## 目的

issue 0065 で compression / content_language / etag / trailer / upgrade / vary の 6 モジュールについてインラインテストを外部化したが、残り 21 ファイルに `#[cfg(test)]` ブロックが残存している。CLAUDE.md の「単体テストのファイル名は `tests/test_<module>.rs` とし、`src/<module>.rs` に対応させること」規約に準拠させる。

また、PBT で既にカバーされているラウンドトリップテストがインラインテストに残っている箇所があり、CLAUDE.md の「PBT でカバーできるものを単体テストで書かない」規約にも違反している。

## 優先度根拠

CLAUDE.md テスト規約違反が 21 ファイルに及ぶ。テストの重複はメンテナンスコストを増加させ、PBT とインラインテストの役割分担が曖昧になる。

## 現状

`src/` 内に `#[cfg(test)]` が残存するファイル (21 件):

| ファイル | 外部テスト存在 | PBT 存在 | 備考 |
|---------|-------------|---------|------|
| `accept.rs` | tests/test_accept.rs | prop_accept.rs | |
| `auth.rs` | tests/test_auth.rs | prop_auth.rs | |
| `base64.rs` | なし | なし | 非公開モジュール。外部化不可 |
| `cache.rs` | tests/test_cache.rs | prop_cache.rs | ラウンドトリップテスト重複あり |
| `conditional.rs` | tests/test_conditional.rs | prop_conditional.rs | |
| `content_disposition.rs` | tests/test_content_disposition.rs | prop_content_disposition.rs | |
| `content_encoding.rs` | tests/test_content_encoding.rs | prop_content_encoding.rs | |
| `content_location.rs` | tests/test_content_location.rs | prop_content_location.rs | |
| `content_type.rs` | tests/test_content_type.rs | prop_content_type.rs | |
| `cookie.rs` | tests/test_cookie.rs | prop_cookie.rs | |
| `date.rs` | tests/test_date.rs | prop_date.rs | |
| `digest_fields.rs` | tests/test_digest_fields.rs | prop_digest_fields.rs | |
| `encoder.rs` | tests/test_encoder/ | prop_encoder.rs | プライベート関数テスト。一部は残す正当性あり |
| `expect.rs` | tests/test_expect.rs | prop_expect.rs | ラウンドトリップテスト重複あり |
| `header_name.rs` | なし | prop_header_name.rs | |
| `host.rs` | tests/test_host.rs | prop_host.rs | |
| `method.rs` | なし | prop_method.rs | |
| `multipart.rs` | tests/test_multipart.rs | prop_multipart.rs | 非公開フィールドアクセス。一部は残す正当性あり |
| `range.rs` | tests/test_range.rs | prop_range.rs | |
| `uri.rs` | tests/test_uri.rs | prop_uri.rs | |
| `validate.rs` | なし | なし | 非公開関数テスト。一部は残す正当性あり |

### 外部化不可/正当な例外

以下はプライベート関数や非公開フィールドへのアクセスが必要なため、インラインテストとして残す正当な理由がある:

- `base64.rs`: 非公開モジュール (`mod base64`)。外部テストからアクセス不可
- `encoder.rs`: `estimate_request_capacity` 等のプライベート関数テスト (`capacity_tests` ブロック) および `validate_response_fields` のプライベート関数テスト (`validate_response_fields_tests` ブロック)。2 つの `#[cfg(test)] mod` が存在する
- `multipart.rs`: `buffer` / `pos` 等の非公開フィールドへの直接アクセス
- `validate.rs`: `escape_quotes` (`pub(crate)`) 等のテスト。integration test からは `pub(crate)` にアクセスできないため残す

### PBT 重複で削除すべきインラインテスト

- `cache.rs:625` `test_cache_control_roundtrip` — `prop_cache.rs` でカバー済み
- `cache.rs:702` `test_age_roundtrip` — `prop_cache.rs` でカバー済み
- `cache.rs:732` `test_expires_roundtrip` — `prop_cache.rs` でカバー済み
- `expect.rs:263` `empty_value_roundtrip` — `prop_expect.rs` でカバー済み

## 設計方針

1. 外部化可能なインラインテストを `tests/test_<module>.rs` に移動する
2. PBT で既にカバーされているラウンドトリップテストは削除する
3. 外部テストが存在しないモジュール (`header_name.rs`, `method.rs`) は `tests/test_header_name.rs`, `tests/test_method.rs` を新設する
4. プライベート関数/非公開フィールドへのアクセスが必要なテストはインラインに残す
5. 外部テストと重複するテストは重複側を削除する

## 実装順序

依存が少なく影響範囲が小さいモジュールから着手する:
1. `header_name.rs`, `method.rs` (外部テスト新設のみ)
2. `content_encoding.rs`, `content_language.rs`, `content_location.rs`, `conditional.rs` (小規模)
3. `cache.rs`, `expect.rs` (PBT 重複削除あり)
4. `accept.rs`, `auth.rs`, `cookie.rs`, `content_type.rs`, `content_disposition.rs`, `digest_fields.rs`, `range.rs`, `host.rs`, `uri.rs`, `date.rs` (残り)

各ファイルの移動時に、`tests/test_<module>.rs` に同等テストが既に存在する場合は重複を解消する (インライン側を削除)。

## 完了条件

- 正当な例外を除き、`src/` 内のインラインテストが `tests/test_<module>.rs` に移動されていること
- PBT 重複のラウンドトリップテストが削除されていること
- インラインテストと `tests/test_<module>.rs` の間の重複が解消されていること
- 既存テスト (PBT / 単体テスト / fuzz) が全て通ること
