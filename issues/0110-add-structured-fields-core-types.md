# RFC 9651 Structured Fields コア型を追加する

- Priority: High
- Created: 2026-06-26
- Completed:
- Model: Kimi K2.7 Code
- Branch: feature/add-structured-fields-core-types
- Polished:

## 目的

RFC 9651 (Structured Field Values for HTTP) で定義される List / Dictionary / Item / Parameters / Inner List などの抽象データ型を、新しい HTTP フィールドのパース・シリアライズに再利用できるようにする。現状は `Accept-Query` 向けに最小限のパーサーしかなく、他の Structured Fields 対応ヘッダーに流用できない。

## 優先度根拠

RFC 9651 は HTTP 拡張ヘッダーで広く利用される基盤仕様であり、`Accept-CH` / `Priority` / `CDN-Loop` 等への対応に先立って汎用モジュールが必要。高い。

## 現状

- `src/accept_query.rs` に `Accept-Query` 専用の List パーサーが存在するが、Integer / Decimal / Boolean / Byte Sequence / Date / Display String / Inner List は拒否 (`UnsupportedItemType`) している
- 汎用的な `structured_fields` モジュールが存在しない
- `src/lib.rs` から Structured Fields 型が公開されていない

## 設計方針

- `src/structured_fields.rs` (または `src/structured_fields/` ディレクトリ) を新設する
- 以下の型を定義する
  - `SfItem`: bare item + parameters
  - `SfList`: `Vec<SfListMember>` (`SfItem` or `SfInnerList`)
  - `SfDictionary`: key -> `SfItem` or `SfInnerList`
  - `SfInnerList`: `Vec<SfItem>` + parameters
  - `SfParameters`: key -> bare item value
  - `SfBareItem`: Integer / Decimal / String / Token / ByteSequence / Boolean / Date / DisplayString
- `no_std` + `alloc` 対応とする
- エラー型は `StructuredFieldsError` を `#[non_exhaustive]` で定義する
- 既存の `accept_query.rs` とは独立して実装し、後続 issue で移行する

## 完了条件

- `src/structured_fields.rs` に RFC 9651 Section 3 の全データ型が定義されていること
- `src/lib.rs` から適切に公開 (`pub mod structured_fields;`) されていること
- 各型に対して `Debug` / `Clone` / `PartialEq` / `Eq` が実装されていること
- 型の構築 API が設計・ドキュメント化されていること

## 解決方法

- 新規モジュールを作成し、RFC 9651 Section 3.1 から 3.3 の各型を Rust 型として表現する
- `SfBareItem` は enum で表現し、各バリアントに対して getter を提供する
- `SfParameters` は重複 key を許容しない (last-wins) 形で保持する
- テストは別 issue で追加する
