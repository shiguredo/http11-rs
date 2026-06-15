# Cache-Control の no-cache / private 修飾が情報喪失する

- Priority: Medium
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-cache-control-no-cache-private-args
- Polished: 2026-06-16

## 目的

`Cache-Control: no-cache="field-name"` および `private="field-name"` の修飾 (qualified form) の引数を保持し、再エンコードできるようにする。

本 issue は 0126 (`cache-control-quoted-comma-split`) と関連する。0126 はディレクティブ分割で `split_with_quotes` を使って quoted-string 内のカンマを区切りと誤認しないようにする bug fix で、本 issue で導入する qualified form (`private="Set-Cookie, Authorization"`) のパースに必要な前提となる。**依存順は 0126 → 本 issue (0125)**。本 issue 着手時点で 0126 がマージ済みであることを完了条件とする。本 issue では `split_with_quotes` への置き換え自体は扱わず (0126 の責務)、no-cache / private 引数の保持と再エンコードに集中する。

## 優先度根拠

Medium とする。RFC 9111 Section 5.2.2.4 / 5.2.2.7 では `no-cache` / `private` は引数なしの形式に加え、`#field-name` 引数を持つ修飾形式を定義している。現状の `CacheControl` は `no_cache` / `private` を `bool` として保持しており、修飾形式の引数が喪失してしまう。

## 現状

- `src/cache.rs` の `CacheControl` は `struct` である。`no_cache` / `private` フィールドは `bool` になっており（76 行目 / 92 行目付近）、修飾形式の引数を保持できない。
- `CacheControl::parse` は `input.split(',')` でディレクティブを分割している（123 行目付近）ため、`private="Set-Cookie, Authorization"` のような quoted-string 内のカンマを誤って区切りと認識する。
- 修飾形式の引数は `no_cache = true` / `private = true` に集約され、引数内容は破棄される（167 行目 / 170 行目 / 176 行目 / 184 行目付近）。
- `fmt::Display` は引数なしの `no-cache` / `private` のみ出力する（372 行目 / 396 行目付近）。

## 設計方針

1. `CacheControl` の `no_cache` / `private` フィールドを enum で表現する。引数なし / 引数あり / 未設定の 3 状態を 1 フィールドで明示的に表現するため、`bool` + `Option<String>` の二重表現は採用しない。

   ```rust
   enum NoCache {
       Disabled,                // 未設定
       Unqualified,             // `no-cache` (引数なし)
       Qualified(Vec<HeaderName>), // `no-cache="field-name, ..."` (引数あり)
   }

   enum Private {
       Disabled,                // 未設定
       Unqualified,             // `private` (引数なし)
       Qualified(Vec<HeaderName>), // `private="field-name, ..."` (引数あり)
   }
   ```

   `Vec<HeaderName>` を採用する理由は (a) RFC 9111 ABNF が `#field-name` (リスト) を規定、(b) `HeaderName` 型 (closed/0091) で field-name の token 制約を型安全に保証、(c) `private="Set-Cookie, Authorization"` のような複数 field-name の自然な表現。

2. 引数 (`Vec<HeaderName>`) は内部で型として保持し、再エンコード時には HeaderName を `escape_quotes` 経由で quoted-string 形式 (`"Set-Cookie, Authorization"`) に整形して出力する。RFC 9111 は token 形式の生成を SHOULD NOT としているため、送信側は quoted-string 形式を用いる。

3. `is_no_cache()` / `is_private()` は `Unqualified` / `Qualified(_)` のいずれの場合も `true` を返す (`Disabled` のときのみ `false`)。

4. 引数付き用の builder (`with_no_cache_args(fields: Vec<HeaderName>)`、`with_private_args(fields: Vec<HeaderName>)`) と getter (`no_cache_args() -> Option<&[HeaderName]>`、`private_args() -> Option<&[HeaderName]>`) を追加する。引数なし用の `with_no_cache()` / `with_private()` はそのまま維持 (`Unqualified` 設定の意味)。

5. ディレクティブ分割の `split_with_quotes` への置き換えは 0126 で扱うため本 issue では行わない。本 issue 着手時点で 0126 がマージ済みであることを前提に、qualified form の引数パース (quoted-string 内のカンマ区切り field-name リストを `Vec<HeaderName>` へ変換) のみ実装する。`parse_quoted_string` (`src/validate.rs`) を内部で利用し、quoted-string を剥がした内部値をカンマでさらに split して field-name に変換する。

6. obs-text (0x80-0xFF) を含む引数は opaque data として保持する (AGENTS.md)。0108 (decoder obs-text) で確立される `bytes_to_opaque_string` の出力ルールと整合する。

7. token 形式 (`no-cache=Set-Cookie` の DQUOTE なし) を受信した場合も同様に保持・再エンコード時に quoted-string 化する。RFC 9111 は受信時に「accept either form」「generate the quoted-string form」を期待しているため、内部表現を quoted-string 経由の `Vec<HeaderName>` に揃える。

## 完了条件

- 0126 (`cache-control-quoted-comma-split`) が完了マージ済みであること (本 issue の前提)。
- `no-cache="Set-Cookie"` / `private="Set-Cookie, Authorization"` がパースされ、`is_no_cache()` / `is_private()` が `true` になること。
- `no_cache_args() -> Option<&[HeaderName]>` / `private_args() -> Option<&[HeaderName]>` で修飾形式の引数 (`Vec<HeaderName>`) を取得できること。
- 上記を再エンコードしたとき、元の quoted-string 形式 (`no-cache="Set-Cookie"` / `private="Set-Cookie, Authorization"`) に戻ること。
- 引数なしの `no-cache` / `private` も引き続きパース・エンコードできること (`Unqualified` ⇔ `no-cache` / `private` の往復)。
- token 形式 (`no-cache=Set-Cookie` の DQUOTE なし) を受信した場合も内部表現に変換でき、再エンコード時に quoted-string 形式で出力すること (RFC 9111 「generate the quoted-string form」)。
- `max-age=3600, private="Set-Cookie"` のように修飾形式と他のディレクティブが混在しても正しく扱えること。
- obs-text を含む field-name (`HeaderName` 構築段階で reject される) は適切にエラーとなること。
- テストが追加されること (`tests/test_cache.rs`):
  - quoted-string 形式 / token 形式の往復
  - 引数なし / 引数あり / 未設定の 3 状態
  - 複数 field-name のパースと再エンコード
  - 他のディレクティブとの混在
- `CHANGES.md` の `## develop` セクションに `[CHANGE]` エントリを `[ADD]` の下、`### misc` の上に追加すること (`CacheControl` の `no_cache` / `private` フィールド型が `bool` から `enum` に変わる破壊的変更のため `[CHANGE]`)。例: `[CHANGE] CacheControl の no_cache / private フィールドを bool から enum (Disabled / Unqualified / Qualified(Vec<HeaderName>)) に変更し、no-cache="field-name" / private="field-name" 修飾形式の引数を保持・再エンコードできるようにする`。

## 解決方法

設計方針に沿って次の順序で実装する。

1. 0126 (`cache-control-quoted-comma-split`) のマージ後に着手する。
2. `src/cache.rs` の `CacheControl` 構造体の `no_cache: bool` / `private: bool` フィールドを `NoCache` / `Private` enum (`Disabled` / `Unqualified` / `Qualified(Vec<HeaderName>)`) に変更する。
3. `CacheControl::parse` (`L114`) の no-cache / private 分岐 (`L167` / `L170` / `L176` / `L184`) を、引数の有無に応じて `Unqualified` / `Qualified` を組み立てる経路に変更する。
4. quoted-string 内の field-name リストは `parse_quoted_string` で剥がしたあとカンマで split し、`HeaderName::try_from` で構築する。token 形式 (DQUOTE なし) も同経路を通す。
5. `with_no_cache` / `with_private` builder は `Unqualified` を設定。`with_no_cache_args` / `with_private_args` builder を追加し `Qualified(Vec<HeaderName>)` を設定。
6. `is_no_cache()` / `is_private()` は `Unqualified` / `Qualified(_)` で `true` を返す。
7. `no_cache_args()` / `private_args()` getter を追加 (`Qualified` のときのみ `Some`)。
8. `fmt::Display` (`L347` / `L372` / `L396` 周辺) で `Qualified` の field-name リストを `escape_quotes` で quoted-string 化する。
9. `tests/test_cache.rs` に完了条件のテストを追加する。
10. `CHANGES.md` に `[CHANGE]` エントリを追加する。

## 参考: RFC 文面

- RFC 9111 Section 5.2.2.4 no-cache
  > The qualified form of the no-cache response directive, with an argument that lists one or more field names, indicates that a cache MAY use the response to satisfy a subsequent request, subject to any other restrictions on caching, if the listed field names are excluded from the subsequent response or the subsequent response has been successfully revalidated with the origin server (updating or removing those fields).
  > The argument form of no-cache uses the quoted-string form of the field value (Section 5.6.4 of [HTTP]), allowing it to contain a comma-separated list of field names. A sender SHOULD NOT generate the token form, even if a recipient is required to parse it for backwards compatibility.
- RFC 9111 Section 5.2.2.7 private
  > The qualified form of the private response directive, with an argument that lists one or more field names, indicates that a shared cache MAY store the response, but MUST do so with the listed header fields removed from the response before any subsequent caching or use.
- RFC 9110 Section 5.6.1 List (`#field-name` の意味)
  > A recipient MUST parse and ignore a reasonable number of empty list elements: enough to handle common mistakes by senders that merge values, but not so much that they could be used as a denial-of-service mechanism.
