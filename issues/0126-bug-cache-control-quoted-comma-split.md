# Cache-Control の quoted-string 内カンマが誤って分割される

- Priority: Medium
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-cache-control-quoted-comma-split
- Polished: 2026-06-16

## 目的

`Cache-Control` ヘッダー値をディレクティブ単位で分割する際に、quoted-string 内のカンマをディレクティブ境界として誤検出しないようにする。

## 優先度根拠

Medium とする。RFC 9111 Section 5.2 は `Cache-Control = #cache-directive` と定義しており、`#rule` は RFC 9110 Section 5.6.1 で展開される。`cache-directive` の引数には `quoted-string` が許容される（RFC 9111 Section 5.2、RFC 9110 Section 5.6.4）。したがって `private="a,b"` は単一のディレクティブである。現状ではカンマで無条件に分割するため `private="a` と `b"` に分断され、`Err(CacheError::InvalidFormat)` になってしまう。

## 現状

- `src/cache.rs` 内 `CacheControl::parse` の line 123 で `for directive in input.split(',')` としており、quoted-string の有無を考慮せずカンマで分割している。
- このため `CacheControl::parse("private=\"a,b\"")` は `Err(CacheError::InvalidFormat)` を返す。

## 設計方針

1. カンマによる分割は `src/validate.rs:431` の `split_with_quotes` を利用する。同関数は `Accept` (`src/accept.rs`) や `Expect` (`src/expect.rs`) でも既に使用されており、quoted-string 内のデリミタを無視し、RFC 9110 Section 5.6.4 の `quoted-pair` に従った `\` によるエスケープも扱える。

   - `split_with_quotes(input, ',')` は DQUOTE を区切り判定のスキップにのみ使い、**戻り値の各要素には DQUOTE がそのまま残る** (validate.rs:431-457)。したがって既存の partial quote 検出 (`cache.rs:134-139`、`stripped.strip_suffix('"').ok_or(...)?`) は置換後もそのまま動作する。

2. ディレクティブ名の大小文字変換や quoted-string からの値取り出しは既存ロジックを維持する。値内カンマが残る場合も、本 issue では `private` / `no-cache` 等の修飾形式を「ディレクティブあり」として扱う既存方針 (`cc.private = true` / `cc.no_cache = true`) を維持し、引数の詳細は保持しない。

3. 本 issue は `private` / `no-cache` 修飾形式の引数そのものを保持する issue (`issues/0125-bug-cache-control-no-cache-private-args.md`) とは **独立した最小修正** とする。引数の詳細な保持は 0125 で対応する。**依存順は本 issue (0126) → 0125** で、0125 は本 issue の `split_with_quotes` 化を前提に動作する。

4. RFC 9110 Section 5.6.1.2 の `#rule` 規定 (`OWS "," OWS`、空リスト要素は `MUST parse and ignore`) を尊重する。空リスト要素処理 (`cache.rs:125 if directive.is_empty() { continue; }`) は既存挙動を維持し、本 issue で変更しない (回帰防止のため既存挙動の固定としてテストを追加する)。

## 完了条件

- `CacheControl::parse("private=\"a,b\"")` が `Ok(...)` を返し、`is_private()` が `true` になること。
- `CacheControl::parse("max-age=60, private=\"a,b\"")` が 2 つのディレクティブとしてパースされ、`max_age() == Some(60)` かつ `is_private() == true` になること。
- 以下のエッジケースを `tests/test_cache.rs` にテストとして追加すること。
  - quoted-string 内カンマを含む単一ディレクティブ (`private="a,b"`)
  - quoted-string 内カンマを含む複数ディレクティブ (`max-age=60, private="a,b"`)
  - カンマ前後の OWS (`max-age=60 , private="a,b" `、RFC 9110 §5.6.1.2 `#rule = [ element ] *( OWS "," OWS [ element ] )` の境界形式)
  - 空リスト要素 (`max-age=60,, private="a,b"`、RFC 9110 §5.6.1.2 「MUST parse and ignore」の固定。既存挙動の回帰防止)
  - quoted-string 内のエスケープされた DQUOTE (`private="a\"b"`) / バックスラッシュ (`private="a\\b"`)
- `CHANGES.md` の `## develop` セクションに以下の `[FIX]` エントリを `[ADD]` の下、`### misc` の上に追加すること (本体ライブラリのバグ修正)。
  - `[FIX] Cache-Control ディレクティブを分割する際に quoted-string 内のカンマを誤って区切っていた問題を修正する`

## 解決方法

設計方針に沿って次の順序で実装する。

1. `src/cache.rs` の use 文を `use crate::validate::{split_with_quotes, trim_ows};` に変更する。
2. `src/cache.rs` の `CacheControl::parse` (`L114-`) で、`input.split(',')` (`L123`) を `split_with_quotes(input, ',')` に置き換える。`split_with_quotes` の戻り値は `Vec<String>` のため、`for directive in split_with_quotes(input, ',') { let directive = trim_ows(&directive); ... }` のように `&` 借用に整える。
3. 既存の partial quote 検出 (`L134-139`) は変更しない (戻り値に DQUOTE が残るため正常動作)。
4. 空リスト要素の `if directive.is_empty() { continue; }` (`L125`) も変更しない。
5. `tests/test_cache.rs` に完了条件のテストを追加する。
6. `CHANGES.md` に `[FIX]` エントリを追加する。

## 参考: RFC 文面

- RFC 9111 Section 5.2 Cache-Control
  > Cache-Control = #cache-directive
  > cache-directive = token [ "=" ( token / quoted-string ) ]
- RFC 9110 Section 5.6.1.2 List (`#rule` 展開)
  > #element => [ element ] *( OWS "," OWS [ element ] )
  > A recipient MUST parse and ignore a reasonable number of empty list elements: enough to handle common mistakes by senders that merge values, but not so much that they could be used as a denial-of-service mechanism.
- RFC 9110 Section 5.6.4 Quoted Strings
  > quoted-string = DQUOTE *( qdtext / quoted-pair ) DQUOTE
  > quoted-pair = "\" ( HTAB / SP / VCHAR / obs-text )
