# URI パーサーが authority なしの入力の先頭 segment のコロンを scheme と誤認する

- Priority: High
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-uri-parser-relative-colon-scheme
- Polished: 2026-06-13

## 目的

`Uri::parse("abc:def")` のように authority マーカー (`//`) を持たず、先頭 path segment に `:` を含む入力を、現状は `scheme() == Some("abc")` / `path() == "def"` として絶対 URI として解釈してしまう。本ライブラリの HTTP 用途では、path-rootless 形式 (`scheme:segment`) の入力は相対参照として扱うため、パース時の誤認を修正する。

RFC 3986 Section 4.2 では relative-path reference の先頭 segment に `:` を含める場合、`./` を前置しなければならない。一方、同じ `abc:def` は path-rootless な絶対 URI としても valid なため、文法上は曖昧である。本 issue では、`:` の後に `/` による path-absolute / authority、または `?` / `#` / 終端による path-empty が続く場合を除き、path-rootless 形式の入力を相対参照として解釈するという本ライブラリの設計選択を `Uri::parse` に反映させる。

## 優先度根拠

High とする。`Uri::parse` は `Content-Location` や `Location` ヘッダー、リクエストターゲットの解決など、相対参照を含む様々な場面で使われる。先頭 segment の `:` を scheme 区切りと誤認すると、`scheme()` / `path()` / `host()` の値が意図と異なり、URI の正規化・解決・再エンコードの整合性を損なう。

## 現状

`src/uri.rs:563-578` の `find_scheme_end` は、`:` より前の文字列が scheme 文字（ALPHA / DIGIT / `+` / `-` / `.`）で構成されていれば、その `:` の位置を scheme 終端として返す。`src/uri.rs:358-472` の `Uri::parse` はこの結果を無条件に採用し、`://` の有無に関わらず scheme を抽出する。

その結果、`Uri::parse("abc:def")` は scheme_end=3 となり、`scheme() == Some("abc")`、`path() == "def"` になる。

一方、`src/uri.rs:906-952` の `build_uri` では、`scheme.is_none() && first_segment_contains_colon(path)` のときに `./` を前置する処理が既に入っている（`issues/closed/0068-fix-uri-normalize-not-idempotent.md` で追加）。これは `normalize` / `resolve` 経由で生成される文字列が再 parse 時に scheme と誤解釈されるのを防ぐ、出力側の回避策である。入力側の誤認はそのまま残っている。

RFC 3986 Section 3.3 (`refs/rfc3986.txt:1227`) の ABNF:

```
path-noscheme = segment-nz-nc *( "/" segment )
segment-nz-nc = 1*( unreserved / pct-encoded / sub-delims / "@" )
              ; non-zero-length segment without any colon ":"
```

RFC 3986 Section 4.2 (`refs/rfc3986.txt:1445-1449`):

> A path segment that contains a colon character (e.g., "this:that") cannot be used as the first segment of a relative-path reference, as it would be mistaken for a scheme name. Such a segment must be preceded by a dot-segment (e.g., "./this:that") to make a relative-path reference.

## 設計方針

1. authority マーカー (`//`) がなく、かつ先頭 path segment に `:` を含む入力は relative-path reference として解析する。
2. 具体的には、`find_scheme_end` で見つかった `:` の直後が `/` (`//` も含む) / `?` / `#` / 入力終端のいずれかであれば絶対 URI として扱い、それ以外 (path-rootless の segment 開始) の場合は、その `:` を scheme 区切りとせず scheme=None として全体を path として扱う。
3. `normalize` / `resolve` 経由で生成される相対参照は、`build_uri` の `./` 前置処理と合わせて一貫して扱う。`build_uri` の `./` 前置は、パース修正後も `normalize` の冪等性を保つために維持する。
4. 既存の `://` 付き絶対 URI（`http://...` 等）には影響を与えない。

## 完了条件

- `Uri::parse("abc:def")` で `scheme() == None` となり、`path() == "abc:def"` となる。
- `Uri::parse("abc:def/ghi?x=1#frag")` でも `scheme() == None` となり、path / query / fragment が正しく取得できる。
- `Uri::parse("http://example.com/path")` 等の `://` 付き絶対 URI は影響を受けない。
- `Uri::parse("/abc:def")` 等、先頭が `/` の path-absolute は影響を受けない。
- `Uri::parse("file:/path")` 等、`:/` 形式の path-absolute な絶対 URI は影響を受けない。
- `Uri::parse(":path")` のような先頭が `:` の入力は、引き続き `scheme() == None` となる。
- `tests/test_uri.rs` に以下の単体テストを追加する（コメント・メッセージは日本語とする）:
  - `test_uri_parse_relative_path_with_colon_first_segment`: `abc:def` が relative-path としてパースされること
  - `test_uri_parse_relative_path_with_colon_and_slashes`: `abc:def/ghi` 等が relative-path としてパースされること
  - `test_uri_parse_relative_path_with_colon_idempotent_normalize`: `abc:def` の `normalize` が冪等であること
- `pbt/tests/prop_uri.rs` に、先頭 segment に `:` を含む scheme-less 文字列が `scheme() == None` となる property を追加する。
- `CHANGES.md` の `## develop` セクションに `[FIX]` エントリを追加する。
- `make fmt && make clippy && make check && make test` が pass する。

## 解決方法

- `src/uri.rs:358-472` の `Uri::parse` で、`find_scheme_end` の結果を採用する条件を追加する。`:` の直後が `/` (`//` も含む) / `?` / `#` / 入力終端でない場合 (path-rootless の segment 開始) は scheme_end を `None` として扱い、パース位置を先頭に戻す。
- `find_scheme_end` 自体は再利用可能なままとし、呼び出し側で曖昧性を解消する。
- `build_uri` の `./` 前置条件を確認し、パース修正後も `normalize` / `resolve` の recomposition が冪等であることを検証する。
- 上記完了条件のテストを追加する。
- `CHANGES.md` に `URI パーサーが authority なしの入力の先頭 segment のコロンを scheme と誤認していた不具合を修正する` 旨を記載する。
