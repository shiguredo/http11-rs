# Accept-Query を RFC 9651 汎用モジュールに移行する

- Priority: Medium
- Created: 2026-06-26
- Completed:
- Model: Kimi K2.7 Code
- Branch: feature/refactor-accept-query-use-structured-fields
- Polished:

## 目的

`src/accept_query.rs` 内の Accept-Query 専用パーサーを、新設される汎用 Structured Fields モジュールを使う形に書き換え、重複実装を削除し保守性を高める。

## 優先度根拠

`Accept-Query` はすでに RFC 9651 に対応しているが、専用実装のため Integer 等の型を無関係に拒否するロジックが重複している。汎用モジュール完成後のリファクタリング。

## 現状

- `src/accept_query.rs` は `AcceptQuery::parse` 内で RFC 9651 Section 4.2.1 / 4.2.3 / 4.2.5 / 4.2.6 / 4.2.3.2 / 4.2.3.3 を独自に実装している
- Token / String / Key / Parameters のみを部分的に実装しており、他の Structured Fields 型は `UnsupportedItemType` として拒否している
- `Display` 実装も独自の SF String / Token 出力ロジックを持っている

## 設計方針

- `AcceptQuery::parse` はまず汎用の `SfList` パーサーを呼び出し、その結果を `Accept-Query` セマンティクスに変換する
- media range 文字列の検証 (`parse_media_range`) と小文字化は `accept_query.rs` に残す
- parameter value の型制限 (String/Token のみ) も `accept_query.rs` 側で実施する
- `Display` 実装も汎用 `SfList` シリアライズを利用する
- 汎用モジュールで実装されている個別の SF 型パース・シリアライズ関数は `accept_query.rs` から削除する

## 完了条件

- `AcceptQuery::parse` が汎用 Structured Fields パーサーを利用して実装されていること
- `AcceptQuery` の `Display` 実装が汎用 Structured Fields シリアライザーを利用していること
- 既存の `tests/test_accept_query.rs` のテストがすべて通ること
- `accept_query.rs` 内の重複した SF パース・シリアライズコードが削除されていること

## 解決方法

- `AcceptQuery::parse` を `SfList::parse` -> `MediaRangeItem` 変換の 2 段階に書き換える
- `parse_item_or_inner_list` / `parse_item` / `parse_bare_item` / `parse_sf_token` / `parse_sf_string` / `parse_parameters` / `parse_key` / `discard_ows` / `discard_sp` / `write_sf_string` / `can_be_token` を削除する
- `AcceptQueryError` は `StructuredFieldsError` への `From` impl またはラップを検討する
