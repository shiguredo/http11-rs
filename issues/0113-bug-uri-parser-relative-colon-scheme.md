# URI パーサーが authority なしの入力の先頭 segment のコロンを scheme と誤認する

- Priority: High
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-uri-parser-relative-colon-scheme
- Polished: 2026-06-16

## 目的

`Uri::parse("abc:def")` のように authority マーカー (`//`) を持たず、先頭 path segment に `:` を含む入力を、現状は `scheme() == Some("abc")` / `path() == "def"` として絶対 URI として解釈してしまう。本ライブラリの HTTP 用途では、path-rootless 形式 (`scheme:segment`) の入力は相対参照として扱うため、パース時の誤認を修正する。

RFC 3986 Section 4.2 では relative-path reference の先頭 segment に `:` を含める場合、`./` を前置しなければならない。一方、同じ `abc:def` は path-rootless な絶対 URI としても valid (RFC 3986 Section 3.3 `path-rootless = segment-nz *( "/" segment )`) なため、文法上は曖昧である。本 issue では、`:` の後に `/` による path-absolute / authority、または `?` / `#` / 終端による path-empty が続く場合を除き、path-rootless 形式の入力を相対参照として解釈するという本ライブラリの **設計選択** を `Uri::parse` に反映させる。

### スコープと意味論変更の明示

本修正は **HTTP / RTSP のメッセージ処理に特化した意味論縮退** であり、汎用 RFC 3986 パーサーとしての挙動から意図的に外れる。具体的な影響:

- `mailto:user@example.com` / `urn:isbn:...` / `tag:example.com,2026:foo` のような path-rootless 形式の absolute URI は、本修正後は `scheme() == None` として相対参照扱いになる。HTTP / RTSP リクエスト経路では `://` 付き絶対 URI (`http://` / `https://` / `rtsp://` / `rtsps://`) のみが流通するため、上記スキームは本ライブラリのスコープ外として明示する
- RTSP のスキーム (`rtsp:` / `rtsps:`) は `://` 付きで来るため本修正の影響を受けない (CLAUDE.md「RTSP/1.0 や RTSP/2.0 も利用できること」と整合)
- 既存の `://` 付き絶対 URI (`http://...` 等) には影響を与えない

この意味論変更は `Uri::parse` の rustdoc および `README.md` の URI 解説節 (存在する場合) に明記し、汎用 RFC 3986 パーサーとして本ライブラリを使うユースケースが存在しないことを利用者へ示す。

## 優先度根拠

High とする。`Uri::parse` は `Content-Location` や `Location` ヘッダー、リクエストターゲット (RFC 9112 Section 3.2 の origin-form / absolute-form) の解決など、相対参照を含む様々な場面で使われる。HTTP/1.1 リクエストの `Location: abc:def` のような Response ヘッダ値、または `Content-Location: abc:def` のような Representation メタデータの解釈で、先頭 segment の `:` を scheme 区切りと誤認すると、`scheme()` / `path()` / `host()` の値が意図と異なり、URI の正規化・解決・再エンコードの整合性を損なう。

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

2. 曖昧性解消は `Uri::parse` 側で行う。`find_scheme_end` の意味論は「scheme syntax として valid な `:` の位置を返す」に保ち変更しない。`Uri::parse` が結果を採用するか否かを以下条件で決める。

   ```rust
   let scheme_end = find_scheme_end(bytes);
   let adopt = match scheme_end {
       Some(colon_pos) => {
           // scheme として採用する条件: : の直後が / / ? / # / 入力終端
           // path-rootless の segment 開始 (例: abc:def の d) の場合は採用しない
           matches!(bytes.get(colon_pos + 1), None | Some(b'/') | Some(b'?') | Some(b'#'))
       }
       None => false,
   };
   let effective_scheme_end = if adopt { scheme_end } else { None };
   ```

3. `Uri::parse` で scheme=None と判定した場合、内部表現として `path` フィールドに **先頭にコロンを含む segment を保持する** (RFC 3986 Section 3.3 `segment-nz-nc` 制約 = relative-path reference の最初の segment にコロン不可 と矛盾するが、本ライブラリの `Uri` 内部表現はこれを許容する設計選択を取る)。`Uri::parse` 側で先頭に `./` を補わない。これは parse → `path()` 取得 → 再構築のラウンドトリップで「parse 時にユーザーが渡した bytes と `path()` の値が一致する」性質を保つため。

   既存の `validate_path` (`src/uri.rs` 内の path 全体検証) は pchar 単位で検査し `path-noscheme` ABNF を強制していない実装と整合するため、本 issue では `validate_path` 側に「先頭 segment にコロンを許す」制約強化は **行わない** (スコープ外)。`validate_path` の挙動変更は別 issue で必要に応じて扱う。

4. `normalize` / `resolve` 経由で出力文字列を生成する `build_uri` (`L916`) の `./` 前置処理 (`L941-952`) は維持する。これは closed/0068 の冪等性確保のために必要。本 issue で `Uri::parse` 側を相対参照扱いに修正しても、`build_uri` で再度 `./` を前置することで `normalize` / `resolve` の出力が再 parse 時に scheme と誤解釈されない性質を保つ。

5. 既存の `://` 付き絶対 URI (`http://...` / `https://...` / `rtsp://...` / `rtsps://...` 等) には影響を与えない。

6. 本 issue による意味論変更を `Uri::parse` の rustdoc に追記する。具体的には「path-rootless 形式の absolute URI (例: `mailto:`、`urn:`、`tag:`) は scheme=None の relative-path reference として解釈される」「本ライブラリは HTTP / RTSP メッセージ処理に特化しており、汎用 RFC 3986 パーサーではない」旨を明記する。

## 完了条件

- `Uri::parse("abc:def")` で `scheme() == None` となり、`path() == "abc:def"` となる。
- `Uri::parse("abc:def/ghi?x=1#frag")` でも `scheme() == None` となり、path / query / fragment が正しく取得できる。
- `Uri::parse("mailto:user@example.com")` は `scheme() == None`、`path() == "mailto:user@example.com"` となる (HTTP / RTSP 用途への意味論縮退の明示)。
- `Uri::parse("http://example.com/path")` 等の `://` 付き絶対 URI は影響を受けない。
- `Uri::parse("rtsp://example.com/stream")` / `Uri::parse("rtsps://example.com/stream")` も影響を受けない。
- `Uri::parse("/abc:def")` 等、先頭が `/` の path-absolute は影響を受けない。
- `Uri::parse("file:/path")` 等、`:/` 形式の path-absolute な絶対 URI は影響を受けない (`scheme() == Some("file")`、`path() == "/path"`)。
- `Uri::parse(":path")` のような先頭が `:` の入力は、引き続き `scheme() == None` となる。
- `Uri::parse("abc:")` のように `:` の直後が入力終端の入力は、設計方針 2 の擬似コードに従い `scheme() == Some("abc")` / `path() == ""` (path-empty を持つ絶対 URI) として扱う。
- `Uri::parse` の rustdoc に以下の文言を追記する (例。表現は微調整可)。

  ```text
  /// HTTP / RTSP メッセージ処理に特化したパーサーであり、汎用 RFC 3986 パーサーではない。
  /// path-rootless 形式の absolute URI (例: `mailto:user@example.com` / `urn:isbn:...` /
  /// `tag:example.com,2026:foo`) は `scheme() == None` の relative-path reference として
  /// 解釈される (本ライブラリのスコープ外スキームのため意図的にこの挙動を取る)。
  /// 影響を受けない: `://` 付き絶対 URI (`http://` / `https://` / `rtsp://` / `rtsps://` 等)、
  /// `/` 始まりの path-absolute、`:/` 形式の path-absolute scheme (`file:/path` 等)、
  /// `:` 単独で始まる入力 (引き続き scheme=None)。
  ///
  /// path-rootless 形式を scheme=None として扱った場合、`path()` は parse 入力をそのまま
  /// 返す (例: `Uri::parse("abc:def").path() == "abc:def"`)。一方で `as_str()` /
  /// `normalize()` / `resolve()` の出力では `build_uri` の `./` 前置処理が働き、
  /// `./abc:def` のように先頭に `./` が付与される (再 parse 時に scheme として誤解釈
  /// されないため。closed/0068 で導入された冪等性確保策と整合する)。
  ```
- `tests/test_uri.rs` に以下の単体テストを追加する (コメント・メッセージは日本語とする):
  - `test_uri_parse_relative_path_with_colon_first_segment`: `abc:def` が relative-path としてパースされること
  - `test_uri_parse_relative_path_with_colon_and_slashes`: `abc:def/ghi` 等が relative-path としてパースされること
  - `test_uri_parse_mailto_path_rootless_is_relative`: `mailto:user@example.com` が scheme=None になることで HTTP/RTSP スコープを示すこと
  - `test_uri_parse_relative_path_with_colon_idempotent_normalize`: `abc:def` の `normalize` が冪等であること (closed/0068 との両立)
- `pbt/tests/prop_uri.rs` に、先頭 segment に `:` を含む scheme-less 文字列が `scheme() == None` となる property を追加する。
- `CHANGES.md` の `## develop` セクションに `[FIX]` エントリを `[ADD]` の下、`### misc` の上に追加する (本体ライブラリのバグ修正)。
- `make fmt && make clippy && make check && make test` が pass する。

## 解決方法

設計方針に沿って次の順序で実装する。

1. `src/uri.rs` の `Uri::parse` (`L358-`) で、`find_scheme_end` の結果採用条件を追加する (設計方針 2 の擬似コード参照)。
2. `find_scheme_end` (`L563-`) 自体は変更しない (scheme syntax 判定の純粋関数として維持)。
3. `Uri::parse` の rustdoc に意味論変更 (HTTP / RTSP 用途への縮退、`mailto:` / `urn:` / `tag:` が scheme=None になる旨) を明記する。
4. `build_uri` (`L916-`) の `./` 前置 (`L941-952`) は変更せず、`normalize` / `resolve` の出力が再 parse 時に scheme と誤解釈されない冪等性を保つ。
5. 上記完了条件のテストを `tests/test_uri.rs` / `pbt/tests/prop_uri.rs` に追加する。
6. `CHANGES.md` の `## develop` セクションに `[FIX] URI パーサーが authority なしの入力の先頭 segment のコロンを scheme と誤認していた不具合を修正する` 旨を記載する。

## 参考: RFC 文面

- RFC 3986 Section 3 (Syntax Components)
  > URI = scheme ":" hier-part [ "?" query ] [ "#" fragment ]
  > relative-ref = relative-part [ "?" query ] [ "#" fragment ]
- RFC 3986 Section 3.1 Scheme
  > scheme = ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )
- RFC 3986 Section 3.3 Path
  > path = path-abempty    ; begins with "/" or is empty
  >      / path-absolute   ; begins with "/" but not "//"
  >      / path-noscheme   ; begins with a non-colon segment
  >      / path-rootless   ; begins with a segment
  >      / path-empty      ; zero characters
  > path-noscheme = segment-nz-nc *( "/" segment )
  > path-rootless = segment-nz *( "/" segment )
  > segment-nz-nc = 1*( unreserved / pct-encoded / sub-delims / "@" )
  >               ; non-zero-length segment without any colon ":"
- RFC 3986 Section 4.2 Relative Reference
  > A path segment that contains a colon character (e.g., "this:that") cannot be used as the first segment of a relative-path reference, as it would be mistaken for a scheme name. Such a segment must be preceded by a dot-segment (e.g., "./this:that") to make a relative-path reference.
- RFC 9112 Section 3.2 Request Target
  > request-target = origin-form / absolute-form / authority-form / asterisk-form
