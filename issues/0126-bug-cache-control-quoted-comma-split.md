# Cache-Control の quoted-string 内カンマが誤って分割される

- Priority: Medium
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-cache-control-quoted-comma-split
- Polished: {YYYY-MM-DD}

## 目的

`Cache-Control` ヘッダー値をカンマで分割する際に、quoted-string 内のカンマを誤ってディレクティブ境界として扱わないようにする。

## 優先度根拠

Medium とする。RFC 9111 Section 5.2 では `Cache-Control` は `*( "," OWS ) cache-directive` で、quoted-string は RFC 9110 の `quoted-string` によりカンマを含みうる。`private="a,b"` が `private="a` と `b"` に分割されると情報が壊れる。

## 現状

- `src/cache.rs` : おそらく先頭からカンマで split している。
- quoted-string 認識がない場合、内部カンマで切られてしまう。

## 設計方針

1. カンマで split する前に quoted-string を認識し、内部カンマをスキップする。
2. または `quoted_string` ユーティリティを利用して正しく token / quoted-string を抽出する。

## 完了条件

- `private="a,b"` が 1 つのディレクティブとして扱われること。
- `max-age=60, private="a,b"` が 2 つのディレクティブとして扱われること。
- テストが追加されること。

## 解決方法

- `src/cache.rs` の分割ロジックを quoted-string 対応に修正する。
- テストを追加する。
