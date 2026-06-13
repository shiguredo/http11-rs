# Scheme / SchemeError の API が Method / HeaderName と非対称

- Priority: Medium
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/refactor-scheme-api-asymmetry
- Polished: {YYYY-MM-DD}

## 目的

`Scheme` / `SchemeError` の API を `Method` / `HeaderName` / `ContentCoding` 等と対称にし、エラー型名を `InvalidScheme` 等に統一する。

## 優先度根拠

Medium とする。他のトークン系型は `InvalidXxx` 形式のエラー名で統一されているが、`SchemeError` のみ `SchemeError` となっており、ユーザーの認知負荷が高い。また `Scheme` の `as_str` メソッド等の命名も他と揃っていない可能性がある。

## 現状

- `src/uri.rs` : `Scheme`, `SchemeError` 定義。
- `src/method.rs` : `Method`, `InvalidMethod`。
- `src/header_name.rs` : `HeaderName`, `InvalidHeaderName`。

## 設計方針

1. `SchemeError` を `InvalidScheme` にリネームする（破壊的変更）。
2. `Scheme::as_str()` 等の命名を他のトークン型と合わせる。
3. `FromStr` 実装や `TryFrom<&str>` の提供状況を他と揃える。

## 完了条件

- `InvalidScheme` 型が定義され、`SchemeError` は削除されるか非推奨エイリアスとなること。
- 全ての利用箇所が `InvalidScheme` に移行されること。
- `Method` / `HeaderName` 等との API 対称性がドキュメントまたはテストで確認されること。

## 解決方法

- `src/uri.rs` の型名とエイリアスを修正する。
- 全ての `SchemeError` 利用箇所を `InvalidScheme` に置き換える。
- `CHANGES.md` に破壊的変更として記載する。
