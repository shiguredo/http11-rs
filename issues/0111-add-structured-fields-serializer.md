# RFC 9651 Structured Fields シリアライズを追加する

- Priority: High
- Created: 2026-06-26
- Completed:
- Model: Kimi K2.7 Code
- Branch: feature/add-structured-fields-serializer
- Polished:

## 目的

RFC 9651 Section 4.1 のシリアライズアルゴリズムを実装し、Structured Fields 型を HTTP ヘッダー値の文字列に変換できるようにする。

## 優先度根拠

パースと対になる出力経路がないと、プロキシ等でヘッダーを書き換えた際に再エンコードできない。Structured Fields 対応の基本機能。

## 現状

- `src/accept_query.rs` には `Display` 実装として `Accept-Query` 専用の簡易シリアライズがあるが、汎用性がない
- 汎用の Structured Fields シリアライザーが存在しない

## 設計方針

- `src/structured_fields.rs` (または `src/structured_fields/serializer.rs`) に実装する
- RFC 9651 Section 4.1.1 から 4.1.11 の各シリアライズ手順をステップ番号をコメントで残しながら実装する
- `Display` または `serialize(&self) -> String` 形式を提供する
- Integer / Decimal / String / Token / Byte Sequence / Boolean / Date / Display String / List / Dictionary / Parameters / Inner List をすべてカバーする
- 出力は US-ASCII のみとする

## 完了条件

- RFC 9651 Section 4.1 の全型に対するシリアライズが実装されていること
- シリアライズ結果が RFC 9651 の ABNF に準拠すること
- 既存テストが通ること

## 解決方法

- `SfList` / `SfDictionary` / `SfItem` / `SfInnerList` / `SfParameters` / `SfBareItem` に対して `Display` impl または `serialize` メソッドを実装する
- エスケープが必要な文字 (DQUOTE, backslash, Display String の `\`) を正しく処理する
- Byte Sequence は Base64 (RFC 4648) でエンコードする
- Date は UNIX 秒を Integer として `@` 形式で出力する
- Display String は `%` 形式で UTF-8 バイト列をエスケープして出力する
