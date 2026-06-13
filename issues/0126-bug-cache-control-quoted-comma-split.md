# Cache-Control の quoted-string 内カンマが誤って分割される

- Priority: Medium
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-cache-control-quoted-comma-split
- Polished: 2026-06-13

## 目的

`Cache-Control` ヘッダー値をディレクティブ単位で分割する際に、quoted-string 内のカンマをディレクティブ境界として誤検出しないようにする。

## 優先度根拠

Medium とする。RFC 9111 Section 5.2 は `Cache-Control = #cache-directive` と定義しており、`#rule` は RFC 9110 Section 5.6.1 で展開される。`cache-directive` の引数には `quoted-string` が許容される（RFC 9111 Section 5.2、RFC 9110 Section 5.6.4）。したがって `private="a,b"` は単一のディレクティブである。現状ではカンマで無条件に分割するため `private="a` と `b"` に分断され、`Err(CacheError::InvalidFormat)` になってしまう。

## 現状

- `src/cache.rs` 内 `CacheControl::parse` の line 123 で `for directive in input.split(',')` としており、quoted-string の有無を考慮せずカンマで分割している。
- このため `CacheControl::parse("private=\"a,b\"")` は `Err(CacheError::InvalidFormat)` を返す。

## 設計方針

1. カンマによる分割は `src/validate.rs` line 431 の `split_with_quotes` を利用する。同関数は `Accept`（`src/accept.rs`）や `Expect`（`src/expect.rs`）でも既に使用されており、quoted-string 内のデリミタを無視し、RFC 9110 Section 5.6.4 の `quoted-pair` に従った `\` によるエスケープも扱える。
2. ディレクティブ名の大小文字変換や quoted-string からの値取り出しは既存ロジックを維持する。ただし、値内カンマが残る場合も `private` / `no-cache` 等の修飾形式は既に「ディレクティブあり」として扱う方針のため、値の詳細は保持しない。
3. 本 issue は `private` / `no-cache` 修飾形式の引数そのものを保持する issue (`issues/0125-bug-cache-control-no-cache-private-args.md`) とは独立した最小修正とする。引数の詳細な保持は 0125 で対応する。

## 完了条件

- `CacheControl::parse("private=\"a,b\"")` が `Ok(...)` を返し、`is_private()` が `true` になること。
- `CacheControl::parse("max-age=60, private=\"a,b\"")` が 2 つのディレクティブとしてパースされ、`max_age() == Some(60)` かつ `is_private() == true` になること。
- 以下のエッジケースを `tests/test_cache.rs` にテストとして追加すること。
  - quoted-string 内カンマを含む単一ディレクティブ
  - quoted-string 内カンマを含む複数ディレクティブ
  - カンマ前後の OWS
  - 空のリスト要素（例: `max-age=60,, private="a,b"`）
  - quoted-string 内のエスケープされた DQUOTE / バックスラッシュ
- `CHANGES.md` の `## develop` セクションに以下の `[FIX]` エントリーを追加すること。
  - `[FIX] Cache-Control ディレクティブを分割する際に quoted-string 内のカンマを誤って区切っていた問題を修正する`

## 解決方法

- `src/cache.rs` の use 文を `use crate::validate::{split_with_quotes, trim_ows};` に変更する。
- `src/cache.rs` の `CacheControl::parse` で、`input.split(',')` を `split_with_quotes(input, ',')` に置き換える。
- `tests/test_cache.rs` に quoted-string 内カンマのテストを追加する。
- `CHANGES.md` に修正内容を記載する。
