# RFC 10008 (QUERY メソッドおよび Accept-Query ヘッダー) のサポートを追加する

- Priority: Medium
- Created: 2026-06-25
- Completed: 2026-06-25
- Model: glm-5.2
- Branch: feature/add-query-method
- Polished: 2026-06-25

## 目的

RFC 10008 (The HTTP QUERY Method, 2026 年 6 月 Standards Track 公開) に準拠し、
QUERY メソッドと Accept-Query ヘッダーのサポートを追加する。

AGENTS.md の「RFC 準拠を最優先すること」に従い、Standards Track として公開された
仕様への対応を行う。RFC 10008 は [STRUCTURED-FIELDS] として RFC 9651
(Structured Field Values for HTTP, RFC 8941 を廃止した最新版) を参照する
(RFC 10008 Section 6.1)。Accept-Query は RFC 9651 Structured Fields の List として
定義されるため、RFC 9651 への準拠も本 issue のスコープに含む。

## 優先度根拠

Medium。

- High ではない理由: 本 issue は標準定数 1 件追加とヘッダーパーサー 1 モジュール
  新設であり、既存 API の破壊的変更を伴わない。QUERY メソッドは `Method` が任意
  token を許容するため現状でも `Method::new("QUERY")` で動作し、Accept-Query も
  新規モジュールのため既存利用者への影響は無い
- Low ではない理由: Standards Track RFC への準拠は AGENTS.md の最優先事項。
  準拠の網羅性を高める観点で放置しない

## 現状

- `src/method.rs:95-104` に `Method::QUERY` 定数が未定義 (`Method` は newtype で
  任意 token を許容するため `Method::new("QUERY")` は現状でも動作するが、
  `GET` / `POST` 等の標準メソッドと同列に const 定数が提供されていない)
- `Accept-Query` ヘッダーのパーサーが未実装 (`src/` 配下で `Accept-Query` /
  `accept_query` / `AcceptQuery` / `QUERY` で grep して 0 件)
- `refs/rfc10008.txt` は配置済み (コミット `01e8a33`)
- `refs/rfc9651.txt` (Structured Fields) は未配置
- 本ライブラリには RFC 9651 Structured Fields の包括的パーサーが存在しない
  (`src/digest_fields.rs` が RFC 9530 向けに簡易 Dictionary パーサーを実装している
  程度。SF List の手本にはならない)
- `skills/shiguredo-http11/SKILL.md:44` の Method 定数一覧に QUERY が未掲載

## 設計方針

### QUERY メソッド定数 (RFC 10008 Section 2)

- `src/method.rs:104` の `PATCH` 定数の後に
  `pub const QUERY: Self = Self::from_static(b"QUERY");` を追加する
- 既存の標準定数 (GET/POST/...) は `/// 標準メソッド定数` の共見出し下にあり
  個別の RFC 節コメントを持たない。QUERY も既存スタイルに合わせて個別コメント
  なしとする
- QUERY は safe / idempotent (RFC 10008 Section 2, 行 223 / 229。
  定義は [HTTP] = RFC 9110 Section 9.2.1 / 9.2.2 に依拠。IANA Table 2 行 493 が
  `Safe=yes, Idempotent=yes` を公式定義する)
- 本ライブラリはパーサー/エンコーダーでありセマンティクス層を扱わないため、
  定数追加のみで十分。RFC 10008 Section 2 の "Servers MUST fail the request if
  the Content-Type request field is missing or is inconsistent" (行 214-216) は
  サーバーセマンティクス層の要件であり本ライブラリのスコープ外
- デコーダー/エンコーダーは token として既に処理可能。request-target form も
  origin/absolute-form で既に許可されるため修正不要

### Accept-Query ヘッダー (RFC 10008 Section 3)

RFC 10008 Section 3 (行 414-452) の規定:

- Accept-Query は **response header field** である (行 414)。Accept (request) と
  方向性が逆だが、API 構成は揃える
- Accept-Query は RFC 9651 Structured Fields の **List** として定義される
  (行 418-422, IANA Table 3 行 509 が `Structured Type = List` を公式定義)
- "Media ranges are represented by a List Structured Header Field of either
  Tokens or Strings, containing the media range value without parameters"
  (行 420-422)。メディアレンジ値本体とパラメータは分かれる
- "Media type parameters, if any, are mapped to Structured Field Parameters
  with the String or Token type. The choice of Token versus String is
  semantically insignificant. That is, recipients MAY convert Tokens to
  Strings, but MUST NOT process them differently based on the received type."
  (行 424-428)
- "Media types do not exactly map to Tokens; for instance, they allow a
  leading digit. In cases like these, the String format needs to be used."
  (行 430-432)。SF Token の先頭は ALPHA or `*` (RFC 9651 Section 4.2.6) のため、
  先頭数字のメディアタイプ (`3gpp/*` 等) は String 必須
- "The only supported uses of wildcards are `*/*`, which matches any type,
  or `xxxx/*`, which matches any subtype of the indicated type." (行 434-435)。
  `*/subtype` は不可
- "The order of types listed in the field value is not significant." (行 437)。
  セマンティクス上は順序無意味だが、Display ラウンドトリップのため入力順序を
  保存する
- "Although the syntax for this field appears to be similar to other fields,
  such as `Accept` (Section 12.5.1 of [HTTP]), it is a Structured Field and
  thus MUST be processed as specified in Section 4 of [STRUCTURED-FIELDS]."
  (行 449-452)。Accept-Query は Accept と類似するが SF であり RFC 9651 Section 4
  に従って MUST 処理される

### RFC 9651 Structured Fields の文字集合・構文 (Accept-Query 実装の核心)

**既存の `src/validate.rs` ヘルパーは SF 経路に再利用不可**。以下の差を明示する。

#### SF Token (RFC 9651 Section 4.2.6)

- 構文: `( ALPHA / "*" ) *( tchar / ":" / "/" )`
- 先頭は ALPHA または `*`。後続は tchar に加え `:` と `/` を許容
- `is_valid_token` (`src/validate.rs:30`) は tchar のみで `/` を拒否し、先頭文字
  制限もないため **SF Token 検証には再利用不可**
- `is_token_char` (`src/validate.rs:11`) は tchar 単体判定のため部分利用できるが、
  `:` と `/` を追加で許容する判定が必要
- **新設関数**: SF Token 検証 (先頭 ALPHA/`*`、後続 tchar/`:`/`/`) を
  `src/accept_query.rs` に実装する
- なお type/subtype に分割した後の各部分検証は HTTP token (tchar only) が
  適切なため、`is_valid_token` を再利用する (Accept の `parse_media_range` と同じ。
  SF Token は `:` と `/` を許容するが、RFC 9110 Section 8.3.1 の type/subtype は
  token (tchar only) のため、`text/html:extra` / `text/html/foo` 等は
  `InvalidMediaRange` とする)

#### SF String (RFC 9651 Section 4.2.5)

- 構文: `DQUOTE *( unescaped / escaped ) DQUOTE`
- `unescaped = %x20-21 / %x23-5B / %x5D-7E` (SP / VCHAR から DQUOTE と backslash
  を除いた範囲)。**HTAB (0x09) も obs-text (0x80-FF) も不許可**
- `escape = "\" ( DQUOTE / "\" )`。`\"` と `\\` のみ。それ以外の `\X` は fail
- `parse_quoted_string` (`src/validate.rs:323`) は HTAB / obs-text / 拡張
  quoted-pair (`\` + HTAB/SP/VCHAR/obs-text) を受理するため **再利用不可**
- **新設関数**: SF String 専用パーサー (%x20-7E のみ、`\"`/`\\` のみエスケープ)
  を `src/accept_query.rs` に実装する

#### SF Parameter key (RFC 9651 Section 4.2.3.3)

- 構文: `( lcalpha / "*" ) *( lcalpha / DIGIT / "_" / "-" / "." / "*" )`
- 先頭は小文字 ALPHA または `*`。**大文字不可**
- `is_valid_token` は大文字を許容するため **再利用不可**
- **新設関数**: SF Parameter key 検証 (lcalpha 系、大文字不可) を
  `src/accept_query.rs` に実装する

#### SF List の空白ルール (RFC 9651 Section 4.2.1)

- メンバー間・パラメータ間の空白は **SP (%x20) のみ**許可。OWS (SP / HTAB) は不許可
- `trim_ows` (`src/validate.rs:396`) は SP / HTAB を両方 trim するため **再利用不可**
- `split_with_quotes` (`src/validate.rs:431`) も HTTP OWS 前提のため **再利用不可**
- **新設関数**: SP 専用の discard / split を `src/accept_query.rs` に実装する

#### RFC 9651 strict processing と空リスト要素

- RFC 9651 Section 1.1 は "the only error handling defined is to fail the
  entire operation altogether" と規定
- trailing comma / 連続カンマ / 任意のパース失敗 / **重複 parameter key** で
  **全体を fail** する (RFC 9651 は Parameters を順序付き map で key は一意とする)
- `src/accept.rs` のような空要素無視 (RFC 9110 Section 5.6.1.2) は行わない
- ただし空入力 (`AcceptQuery::parse("")`) は空リストとして受理する
  (RFC 9651 Section 3.1: 空 List はフィールド不在で表現されるが、パーサーとしては
  空値を空リスト扱いする。`src/accept.rs` の `Accept::parse("")` と整合)

#### RFC 9651 は全 ASCII / obs-text 不可

- RFC 9651 Section 4.2 step 1 は "Convert input_bytes into an ASCII string;
  if conversion fails, fail parsing"
- SF の全経路 (Token / String / Parameter key / Parameter value) で非 ASCII は fail
- AGENTS.md の obs-text 規約 (obs-text を許容する全経路) は RFC 9110 quoted-string
  系経路のみに適用され、**SF 経路は対象外**

### Accept-Query のパース方針

- 新規モジュール `src/accept_query.rs` に最小実装する
  - `src/digest_fields.rs` は RFC 9651 Dictionary に完全準拠していない簡易実装
    (空要素無視、OWS 許容) であり、SF List の手本にならない。RFC 9651
    Section 4.2.1 に従って新規実装する
  - 汎用 RFC 9651 パーサーは新設しない (依存最小主義、YAGNI)。将来 SF ヘッダーが
    3 つ以上になった時点で共通パーサーの抽出を再検討する
- List メンバーは Token または String (RFC 10008 Section 3)。Inner List は不可
- メディアタイプパラメータは SF Parameters で表現。値は String または Token のみ
  (RFC 10008 Section 3)。Boolean true (`;key` 値なし) は String/Token でないため
  **reject** し `InvalidParameter` を返す
- ワイルドカードは `*/*` と `xxxx/*` のみ。`*/subtype` は `InvalidMediaRange`
- media type / subtype は小文字に正規化する (Accept の `parse_media_range` と整合)
- Token と String は semantically insignificant だが、Display ラウンドトリップの
  ため **入力順序を保存** する

### Display ラウンドトリップの方針

- Display は Token 形式を優先する (Token で表現可能な media range は Token で出力)
- 先頭が ALPHA/`*` の media range は Token 形式、それ以外 (先頭数字等) は
  String 形式で Display 出力する
- パラメータ値も Token で表現可能なら Token、それ以外は String で出力する
- パラメータセパレータは `;` (SP なし) とする (RFC 10008 の例 行 447
  `application/sql;charset="UTF-8"` に合わせる。`src/accept.rs` の `"; {}={}"`
  とは異なるので注意)
- 空パラメータ値 (`charset=""`) は SF Token が 1 文字以上 (先頭 1 文字必須) の
  ため Token 不可、SF String `""` で Display する
- ラウンドトリップは **意味等価** (`parse(Display(x)) == x` の構造的等価) で
  検証する。入力の Token/String 判別は保存せず、Display で Token に正規化する
  (RFC 10008 Section 3 の "MAY convert Tokens to Strings" と
  "MUST NOT process them differently based on the received type" に従う)
- `MediaRangeItem` のデータモデルは `Vec<(String, String)>` (Token/String 区別を
  保持しない) とし、`src/accept.rs` の `MediaRange` と API 構成を揃える
  - `MediaRange` と同名にしない理由: q 値を持たず、parameter 表現も SF Parameters
    (String/Token) と HTTP parameters で文字集合が異なるため、構造の異なる型を
    同名にすると誤用を生む。`MediaRangeItem` として区別する

### API 一貫性

`AcceptQuery` / `MediaRangeItem` / `AcceptQueryError` は `src/accept.rs` の
`Accept` / `MediaRange` / `AcceptError` と API 構成を揃える:

- `parse(&str) -> Result<Self, AcceptQueryError>`
- `items() -> &[MediaRangeItem]`
- `Display` 実装でラウンドトリップ可能
- `AcceptQueryError` は `#[non_exhaustive]` + `core::error::Error` 実装

`AcceptQueryError` のバリアント:

- `InvalidFormat` - trailing comma / 未消費残り文字 / 連続カンマ 等
- `InvalidMediaRange` - type/subtype 形式でない / `*/subtype` / type/subtype が
  HTTP token として不正な文字を含む (`:` / 追加の `/` 等) / SF Token として
  不正な文字 を含む media range
- `InvalidParameter` - SF Parameter key 不正 / 値なし (Boolean true) / parameter value
  が String/Token 以外 (`?1` / `?0` / Integer 等の明示的 Boolean・数値) / 重複
  parameter key (`;key=v1;key=v2`)
- `UnterminatedQuote` - SF String の閉じ DQUOTE が見つからない
- `UnsupportedItemType` - Integer / Decimal / Boolean / Byte Sequence / Date /
  Display String / Inner List (Token/String 以外の SF 型。**リストメンバーの型**
  専用。parameter value の型違反は `InvalidParameter`)

`UnsupportedItemType` を単一バリアントに集約する理由: Accept-Query は
Token/String のみを許可し、それ以外は全て「サポート対象外」として一律拒否する。
個別バリアント (`UnsupportedInteger` 等) に分けると Accept-Query の用途上
デバッグ性への寄与が薄く、エラー型の肥大化を招くため集約する。

### refs/rfc9651.txt の配置

- AGENTS.md「RFC を確認する際は refs/ 以下を利用すること」に従い
  `refs/rfc9651.txt` を配置する
- 取得元: `https://www.rfc-editor.org/rfc/rfc9651.txt`
- **配置後に RFC 9651 の節番号を実際に確認してから実装する**。本 issue に記載
  した節番号 (Section 4.2.1 等) は推定であり、配置後に検証すること

## 検討したが不採用の方針

### A. 汎用 RFC 9651 Structured Fields パーサーを新設する

- `src/structured_fields.rs` 等を汎用パーサーとして新設し、`accept_query.rs` と
  `digest_fields.rs` から利用する方針
- 不採用理由: 依存最小主義・YAGNI。現状 SF ヘッダーは `digest_fields` (Dictionary)
  と `accept_query` (List) の 2 つのみ。共通パーサーの抽出は将来 SF ヘッダーが
  3 つ以上になった時点で再検討する

### B. `src/digest_fields.rs` の SF パーサーを拡張して List に対応する

- `digest_fields.rs` の簡易 Dictionary パーサーを拡張し、List 構文も扱う方針
- 不採用理由: `digest_fields.rs` は RFC 9651 Dictionary に完全準拠していない簡易
  実装 (空要素無視 = `if part.is_empty() { continue; }`、OWS 許容 = `trim_ows`)
  であり、これを拡張して List 構文を足すと更に複雑化し、両ヘッダーの関心事が
  混在する

### C. `src/accept.rs` の `#rule` パーサーを流用する

- `split_with_quotes` / `parse_media_range` 等を流用し、SF List を `#rule` として
  パースする方針
- 不採用理由: RFC 10008 Section 3 (行 449-452) が "MUST be processed as specified
  in Section 4 of [STRUCTURED-FIELDS]" と規定。`#rule` は RFC 9110 の構文であり、
  SF List (SP のみ空白許可、trailing comma fail 等) と文字集合・構文が異なる

## 完了条件

### 実装

- `Method::QUERY` 定数が `src/method.rs:104` の次行に追加されている
  (既存スタイルに合わせ個別コメントなし)
- `src/accept_query.rs` に `AcceptQuery` / `MediaRangeItem` / `AcceptQueryError` が
  実装され、`src/lib.rs` で `pub mod accept_query;` として `accept` の直後に
  公開されている
- SF Token / SF String / SF Parameter key / SF List 空白ルールの各検証関数が
  `src/accept_query.rs` に新設されている (SF Token 検証に `is_valid_token` /
  `parse_quoted_string` / `trim_ows` / `split_with_quotes` は再利用していない。
  ただし type/subtype の HTTP token 検証には `is_valid_token` を再利用する)
- `refs/rfc9651.txt` が `https://www.rfc-editor.org/rfc/rfc9651.txt` から取得されて
  配置されている。配置後に RFC 9651 の節番号を実確認し、実装が一致している

### テスト

- `tests/test_accept_query.rs` にユニットテストが追加されている
  - RFC 10008 の例 (Section 3 行 447 / Appendix A.3 行 672 / A.5 行 835 /
    A.6 行 1036) を網羅。A.3 は Token 形式の唯一の例、A.5/A.6 は String 形式
  - Display ラウンドトリップ (Token/String 正規化、先頭数字 media range の
    String 出力)
  - エラーケース: trailing comma / 連続カンマ / Inner List / Integer / Decimal /
    Boolean / Byte Sequence (`:base64:`) / Date (`@1234567890`) / Display String /
    `*/subtype` / type/subtype が HTTP token として不正な文字を含む
    (`text/html:extra` / `text/html/foo` 等) / SF String 内 HTAB / SF String 内
    obs-text / SF String 不正エスケープ (`\a`) / SF Parameter key 大文字 /
    値なしパラメータ (`;key`) / 明示的 Boolean parameter value (`;key=?1`) /
    重複 parameter key (`;key=v1;key=v2`) / 未消費残り文字
  - 空入力 `AcceptQuery::parse("")` が空リストを返す
- `pbt/tests/prop_accept_query.rs` に PBT が追加されている
  - Display ラウンドトリップ (意味等価 `parse(Display(x)) == x`)
  - アクセサ (media_type / subtype / parameters)
  - パラメータ付きラウンドトリップ
  - String で表現した media range のラウンドトリップ
    - Token 表現可能な media range をあえて String で渡す (`"text/html"`)
    - 先頭数字の type で Token 表現不可能な media range を String で渡す
      (`"3gpp/*"`)。Display が String 形式で出力される経路を検証する
  - strategy は SF Token として妥当な文字集合 (先頭 ALPHA/`*`) に制限する。
    String-only 表現 (先頭数字の type) の strategy も別途定義する。
    parameter key strategy は lcalpha + `*` 先頭、`_-.*` と DIGIT 後続。
    parameter value strategy は SF Token (先頭 ALPHA/`*`) または SF String
    (%x20-7E、空文字列含む) を生成する
- `pbt/tests/prop_request.rs` / `pbt/tests/prop_encoder.rs` /
  `pbt/tests/prop_decoder/main.rs` の `http_method()` strategy に
  `Method::QUERY` を追加する (QUERY + Content-Length / QUERY + chunked の
  エンコード/デコード経路を PBT で検証するため)
- `fuzz/fuzz_targets/fuzz_accept_query.rs` を新設し、任意バイト列入力 →
  `AcceptQuery::parse` が panic / abort しないことを検証する。
  parse 成功時は `items()` / `media_type()` / `subtype()` / `parameters()`
  アクセサを呼び出し、Display 出力を再パースしてラウンドトリップを検証する
  (`fuzz/fuzz_targets/fuzz_accept.rs` / `fuzz_digest_fields.rs` と同様の慣例。
  `fuzz/Cargo.toml` への target 登録を含む)

### ドキュメント・サンプル

- `examples/http11_reverse_proxy/src/main.rs:589` の CONNECT 拒否時 `Allow`
  ヘッダーに `QUERY` を追加する。追加後の値は
  `"GET, HEAD, POST, PUT, DELETE, OPTIONS, PATCH, QUERY"` とする
  - QUERY は safe/idempotent で reverse proxy が upstream に転送するため Allow
    に含める。TRACE は loop-back 診断用で proxy 転送対象外のため含めない
    (既存の TRACE 除外方針を維持)
- `README.md` の規格書一覧 (行 545-568) に以下を追加する (既存形式に合わせ
  タイトル + datatracker URL の二行形式):
  - `RFC 9651 - Structured Field Values for HTTP`
    - `<https://datatracker.ietf.org/doc/html/rfc9651>`
  - `RFC 10008 - The HTTP QUERY Method`
    - `<https://datatracker.ietf.org/doc/html/rfc10008>`
  - 挿入位置: RFC 9530 の次に RFC 9651、その次に RFC 10008 (番号順。
    現状は RFC 9112 → RFC 9530 の順で昇順に並んでいるため、RFC 9530 の後に挿入する)
- `README.md` の「その他のヘッダー」セクション (行 375-404) に Accept-Query
  を追加する (Content-Digest / Repr-Digest 等と同列)
- `skills/shiguredo-http11/SKILL.md` の以下を更新する:
  - 行 44 の Method 定数一覧に `Method::QUERY` を追加する
  - ヘッダーパースモジュールテーブル (行 136-158) に `accept_query` 行を追加する
    (`AcceptQuery` / `MediaRangeItem` / `AcceptQueryError` / RFC 10008 / RFC 9651)
  - RFC 準拠テーブル (行 532-545) に RFC 9651 / RFC 10008 行を追加する
- `CHANGES.md` の `## develop` に以下の ADD エントリを記載する:
  ```
  - [ADD] QUERY メソッド (RFC 10008) と Accept-Query ヘッダー (RFC 10008 Section 3 / RFC 9651 Structured Fields) のサポートを追加する
    - `Method::QUERY` 定数を追加する (safe, idempotent)
    - `accept_query::AcceptQuery` / `accept_query::MediaRangeItem` / `accept_query::AcceptQueryError` を追加する
    - Accept-Query は RFC 9651 Structured Fields の List としてパースする (Token / String のみ、Inner List 不可)
    - @voluntas
  ```

### 後方互換性

- `Method::QUERY` 定数追加: 非破壊 (新規定数のみ)
- `accept_query` モジュール新設: 非破壊 (新規モジュールのみ)
- `Allow` ヘッダーへの QUERY 追加: 405 レスポンスの内容変更だが、破壊的影響はない
  (QUERY を転送可能なことを示すのみ)

### 検証コマンド

- `cargo test --workspace --all-targets` がすべて PASS すること
- `cargo clippy --workspace --all-targets -- -D warnings` が PASS すること
- `cargo fmt --all -- --check` が PASS すること

## 解決方法

1. `refs/rfc9651.txt` を `https://www.rfc-editor.org/rfc/rfc9651.txt` から取得して
   配置した。RFC 9651 Section 4.2 系の節番号を実確認し、実装が一致していることを
   検証した
2. `src/method.rs` の `PATCH` 定数の次行に
   `pub const QUERY: Self = Self::from_static(b"QUERY");` を追加した
   (既存スタイルに合わせ個別コメントなし)
3. `src/accept_query.rs` を新設した
   - RFC 9651 Section 4.2.1 (Parsing a List) / 4.2.3 (Item) / 4.2.3.2 (Parameters)
     / 4.2.3.3 (Key) / 4.2.5 (String) / 4.2.6 (Token) に従う最小パーサー
   - SF Token 検証 (先頭 ALPHA/`*`、後続 tchar/`:`/`/`) を新設
     (type/subtype 検証には `is_valid_token` を再利用)
   - SF String パーサー (%x20-7E のみ、`\"`/`\\` のみエスケープ) を新設
   - SF Parameter key 検証 (lcalpha 系、大文字不可) を新設
   - SP 専用の discard / OWS (SP/HTAB) 専用の discard 関数を新設
   - `AcceptQueryError` は `InvalidFormat` / `InvalidMediaRange` /
     `InvalidParameter` / `UnterminatedQuote` / `UnsupportedItemType` を持つ
   - `MediaRangeItem` は `media_type` / `subtype` / `parameters` を持ち、
     Token/String 区別は保持しない (`Vec<(String, String)>`)
   - Display は Token 形式を優先し、先頭数字等の Token 不可な media range は
     String 形式で出力する。空パラメータ値は SF String `""` で出力する
   - 重複 parameter key は RFC 9651 Section 4.2.3.2 step 7 に従い
     最後の値で上書き (last-wins) する
     (レビューで指摘された RFC 準拠違反を修正)
4. `src/lib.rs` に `pub mod accept_query;` を追加した (`accept` の直後)
5. `tests/test_accept_query.rs` を作成した
   (RFC 10008 の例 4 件、Display ラウンドトリップ、エラーケース全網羅、空入力、
   重複 parameter key の last-wins 挙動)
6. `pbt/tests/prop_accept_query.rs` を作成した
   (String-only 表現の strategy も別途定義)
7. `pbt/tests/prop_request.rs` / `pbt/tests/prop_encoder.rs` /
   `pbt/tests/prop_decoder/main.rs` の `http_method()` に `Method::QUERY` を追加した
8. `fuzz/fuzz_targets/fuzz_accept_query.rs` を新設し、`fuzz/Cargo.toml` に登録した
   (parse 成功時はアクセサ呼び出し + Display ラウンドトリップを検証する)
9. `examples/http11_reverse_proxy/src/main.rs` の `Allow` ヘッダーに
   `QUERY` を追加した
   (`"GET, HEAD, POST, PUT, DELETE, OPTIONS, PATCH, QUERY"`)
10. `README.md` の規格書一覧に RFC 9651 / RFC 10008 を追加した (番号順)。
    「その他のヘッダー」セクションに Accept-Query を追加した
11. `skills/shiguredo-http11/SKILL.md` を更新した:
    - Method 定数一覧に `Method::QUERY` を追加した
    - ヘッダーパースモジュールテーブルに `accept_query` 行を追加した
    - RFC 準拠テーブルに RFC 9651 / RFC 10008 行を追加した
12. `CHANGES.md` の develop に ADD エントリを記載した
13. `cargo test --workspace --all-targets` /
    `cargo clippy --workspace --all-targets -- -D warnings` /
    `cargo fmt --all -- --check` で検証した
