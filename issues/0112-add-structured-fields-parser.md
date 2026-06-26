# RFC 9651 Structured Fields パースを追加する

- Priority: High
- Created: 2026-06-26
- Completed:
- Model: Kimi K2.7 Code
- Branch: feature/add-structured-fields-parser
- Polished:

## 目的

RFC 9651 Section 4.2 のパースアルゴリズムを実装し、HTTP ヘッダー値の文字列から Structured Fields 型を構築できるようにする。

## 優先度根拠

RFC 9651 対応の核心機能。`Accept-Query` 以外の Structured Fields ヘッダー (`Accept-CH`, `Priority` 等) を受信側で処理するために必須。

## 現状

- `src/accept_query.rs` に `Accept-Query` 専用の List パーサーがあるが、Integer / Decimal / Boolean / Byte Sequence / Date / Display String / Inner List を拒否している
- 汎用的な Structured Fields パーサーが存在しない

## 設計方針

- `src/structured_fields.rs` (または `src/structured_fields/parser.rs`) に実装する
- RFC 9651 Section 4.2.1 から 4.2.10 の各パース手順をステップ番号をコメントで残しながら実装する
- 入力は ASCII string として扱い、非 ASCII バイト (0x80-0xFF) は `InvalidInput` で拒否する (RFC 9651 Section 4.2 step 1)
- パースは strict: 仕様に書かれた手順と異なる寛容な解釈は行わない
- List / Dictionary / Item のトップレベルパース関数を提供する

## 完了条件

- RFC 9651 Section 4.2 の全型に対するパースが実装されていること
- パース結果が RFC 9651 の ABNF に準拠する入力を受理し、不適合な入力を拒否すること
- 単体テストで主要な成功／失敗パターンをカバーすること

## 解決方法

- `parse_list(&str) -> Result<SfList, StructuredFieldsError>` 等の関数を実装する
- `parse_item_or_inner_list` / `parse_bare_item` / `parse_parameters` / `parse_key` 等の共通ヘルパーを実装する
- Integer / Decimal は桁数・範囲制限 (RFC 9651 Section 3.3.1 / 3.3.2) を守る
- Byte Sequence は Base64 decode を行う
- Date は `@` 形式の UNIX 秒を解析する
- Display String は `%` 形式の UTF-8 バイト列を decode する
