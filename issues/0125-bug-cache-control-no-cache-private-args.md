# Cache-Control の no-cache / private 修飾が情報喪失する

- Priority: Medium
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-cache-control-no-cache-private-args
- Polished: {YYYY-MM-DD}

## 目的

`Cache-Control: no-cache="field-name"` および `private="field-name"` の field-name 引数を保持し、再エンコードできるようにする。

## 優先度根拠

Medium とする。RFC 9111 Section 5.2.2.7 / 5.2.2.9 では `no-cache` / `private` に `=` による quoted-string 引数を取れる。現状の `CacheControl` 表現では `NoCache` / `Private` が引数なしのバリアントであり、引数が喪失する。

## 現状

- `src/cache.rs` : `CacheControl` enum。
- `NoCache` / `Private` が unit バリアント。

## 設計方針

1. `NoCache(Option<QuotedString>)` / `Private(Option<QuotedString>)` のように引数を保持する。
2. 引数がない場合は `None` とする。
3. 再エンコード時に引数を正しく出力する。

## 完了条件

- `no-cache="Set-Cookie"` / `private="Set-Cookie, Authorization"` がパースされ、再エンコードで元の形式に戻ること。
- 引数なしの `no-cache` / `private` も引き続き扱えること。
- テストが追加されること。

## 解決方法

- `src/cache.rs` の enum バリアントを修正する。
- パーサーとエンコーダーを更新する。
- テストを追加する。
