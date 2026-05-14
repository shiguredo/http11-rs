# 0061 fix accept / content_type / expect の parse_quoted_string に qdtext / quoted-pair 文字種検証を追加する

Created: 2026-05-14
Completed: 2026-05-14
Model: deepseek-v4-pro
Priority: High

## 経緯

本 issue は `issues/closed/0059-fix-quoted-string-latin1-mojibake.md` の後続 issue である。旧 0061 は番号衝突解消のため 0067 にリネーム済み。

## 概要

`src/accept.rs`、`src/content_type.rs`、`src/expect.rs` の 3 モジュールに存在する `parse_quoted_string` 関数は `char_indices()` ベースの走査に移行済み (issue 0059 でスコープ外とされた残件) だが、qdtext / quoted-pair の文字種検証を一切行っていない。このため CR (0x0D)、LF (0x0A)、NUL (0x00) を含む制御文字が quoted-string 値として保存される。

`src/auth.rs` と `src/content_disposition.rs` の quoted-string パーサーは既に `validate.rs` の `is_qdtext_char` / `is_quoted_pair_char` による検証済みである。本 issue では残る 3 モジュールに同様の検証を追加する。

## 根拠

### RFC 9110 Section 5.6.6 (Parameters)

```
parameter-value = ( token / quoted-string )
```

Accept / Content-Type / Expect のパラメータ値は上記定義に従い `quoted-string` 構文を取り得る (`refs/rfc9110.txt:1818-1821`、Section 5.6.6 本文 1811-1834 行)。

### RFC 9110 Section 5.6.4 (Quoted Strings)

```
quoted-string  = DQUOTE *( qdtext / quoted-pair ) DQUOTE
qdtext         = HTAB / SP / %x21 / %x23-5B / %x5D-7E / obs-text
quoted-pair    = "\" ( HTAB / SP / VCHAR / obs-text )
```

(`refs/rfc9110.txt:1786-1794`) これらの ABNF は CR / LF / NUL および他の CTL (%x01-0x08, %x0B-%x0C, %x0E-%x1F, %x7F DEL) を禁止している。

### RFC 9110 Section 5.5 (Field Values)

`refs/rfc9110.txt:1606-1615`:

- CR / LF / NUL を含む field value に対して MUST reject or replace with SP
- 他の CTL 文字は "also invalid; however, recipients MAY retain such characters for the sake of robustness when they appear within a safe context" (e.g., application-specific quoted string)
- Accept / Content-Type / Expect の quoted-string は HTTP インターミディアリが解釈・書換する標準ヘッダであり safe context に該当しないため、他の CTL も保守的に reject する

### obs-text の扱い

`validate.rs` の `is_qdtext_char` / `is_quoted_pair_char` は RFC 9110 Section 5.5 の「recipient SHOULD treat obs-text as opaque data」に基づき、`U+0080..=U+10FFFF` (surrogate 除く) の Unicode scalar を受理する拡張解釈を採用している (AGENTS.md `### RFC について` 節・issue 0059 で確立)。3 モジュールの修正後もこの挙動を一貫して維持する。

### 内部実装: validate.rs の参照

- `pub(crate) fn is_qdtext_char(c: char) -> bool` (`src/validate.rs:272-274`) — qdtext 文字判定
- `pub(crate) fn is_quoted_pair_char(c: char) -> bool` (`src/validate.rs:285-287`) — quoted-pair 右辺文字判定

## 再現手順

1. Accept: `Accept::parse("text/html; charset=\"\r\n\"")` が制御文字 `\r\n` を含む quoted-string を受理する（注: `q=` は `QValue::parse` が先に失敗するため拡張パラメータで再現）
2. Content-Type: `ContentType::parse("text/html; charset=\"\r\n\"")` が制御文字を含む quoted-string を受理する
3. Expect: `Expect::parse("foo=\"\r\n\"")` が token=quoted-string 経路で制御文字を受理する (`Expect` は token-only / token=value のいずれかの形式が必要であり、`\"` で始まる入力は token として reject されるため `parse_quoted_string` に到達しない)
4. 受理された値が上位アプリで再生成されると HTTP Response Splitting (CWE-113) の経路となる

## 対象ファイル

- `src/accept.rs` の `parse_quoted_string` 関数 — 呼び出し元: `parse_param_value`
- `src/content_type.rs` の `parse_quoted_string` 関数 — 呼び出し元: `parse_parameters`
- `src/expect.rs` の `parse_quoted_string` 関数 — 呼び出し元: `parse_value`

修正対象外:
- `src/accept.rs` と `src/expect.rs` の `split_with_quotes` — 分割が責務であり、文字種検証は `parse_quoted_string` に委譲するため変更不要

## 推奨対応

### 共通実装の集約 (3 モジュール統一)

`src/validate.rs` に共通関数 `parse_quoted_string` と `enum QuotedStringError` を `pub(crate)` で置き、3 モジュールから呼び出す。

```rust
pub(crate) enum QuotedStringError {
    InvalidQdtext,
    InvalidQuotedPair,
    Unterminated,
}

pub(crate) fn parse_quoted_string(input: &str) -> Result<(String, &str), QuotedStringError>;
```

各モジュールは `From<QuotedStringError>` を実装してエラー型へマップする:

| モジュール | 文字種不正 | 終端引用符なし |
|---|---|---|
| `accept.rs` | `AcceptError::InvalidParameter` | `AcceptError::UnterminatedQuote` (新規バリアント) |
| `content_type.rs` | `ContentTypeError::InvalidParameter` | `ContentTypeError::UnterminatedQuote` (既存) |
| `expect.rs` | `ExpectError::InvalidValue` | `ExpectError::UnterminatedQuote` (新規バリアント) |

`AcceptError::UnterminatedQuote` / `ExpectError::UnterminatedQuote` は破壊的追加 (`#[non_exhaustive]` 下では match 利用側に新規 variant の追加が必要)。CHANGES.md に `[CHANGE]` エントリを記載する。

## テスト戦略

### 単体テスト

各テストで CR / LF / NUL および他の CTL (0x01-0x08, 0x0B-0x0C, 0x0E-0x1F, 0x7F DEL) を含む quoted-string が reject されることと、有効な VCHAR / HTAB / SP / obs-text が引き続き受理されることを確認する。CTL reject は各モジュールのテスト関数に全 CTL 文字を列挙する形でまとめる。

`tests/test_accept.rs`:
- `test_accept_quoted_string_rejects_ctl` — qdtext / quoted-pair 経路で CR / LF / NUL / 他の CTL が `InvalidParameter` で reject
- `test_accept_quoted_string_accepts_obs_text` — obs-text (U+0080 以上) を含む quoted-string が受理

`tests/test_content_type.rs`:
- `test_content_type_quoted_string_rejects_ctl` — qdtext / quoted-pair 経路で CTL が reject。終端引用符なし + CTL 混在 (`"\r\n`) で `InvalidParameter` (UnterminatedQuote より優先) を確認
- `test_content_type_quoted_string_accepts_obs_text` — obs-text 受理

`tests/test_expect.rs`:
- `test_expect_quoted_string_rejects_ctl` — qdtext / quoted-pair 経路で CTL が `InvalidValue` で reject
- `test_expect_quoted_string_accepts_obs_text` — obs-text 受理
- `test_expect_empty_quoted_string` — 空 quoted-string `""` が引き続き受理される (既存リグレッション防止)。accept / content_type も空 quoted-string のリグレッション防止テストを追加する

### PBT

`pbt/src/lib.rs` に `qdtext_char()` / `qdtext_value(range)` を共通化し、各 PBT ファイルから呼び出す。`prop_content_disposition.rs` 既存の `qdtext_char()` も同経路に置き換える。

- `pbt/tests/prop_accept.rs`: `prop_accept_quoted_obs_text_roundtrip` を追加し、obs-text を含む quoted parameter のラウンドトリップ + Display 出力に value がそのまま埋め込まれることを assert する。
- `pbt/tests/prop_content_type.rs`: 同形式の `prop_content_type_quoted_obs_text_roundtrip` を追加。
- `pbt/tests/prop_expect.rs`: 同形式の `prop_expect_quoted_obs_text_roundtrip` を追加。3 モジュール一律で obs-text の網羅検証を担保する。

### Fuzzing

- 既存の `fuzz_accept.rs`、`fuzz_content_type.rs`、`fuzz_expect.rs` は任意 UTF-8 入力でパースを試行する。修正後は reject パスが増えるのみで panic 安全性上の変化はない。変更不要。
- 3 ターゲットとも `cargo +nightly fuzz run <target> -- -max_total_time=60` で新規 crash が無いことを確認する。特に `fuzz_content_type.rs` は Display ラウンドトリップ assertion (line 36-37) が stricter validation 後も破綻しないことを確認する。

## エッジケース

| ケース | 修正後挙動 | 備考 |
|---|---|---|
| 空 quoted-string `""` | 即座に `("", rest)` を返す。Display 出力も `name=""` (引用符付き) で再パース可能 | `needs_quoting("")` を `true` に修正済 |
| obs-text (U+0080..=U+10FFFF) | `is_qdtext_char` / `is_quoted_pair_char` が受理 | opaque 保持、変更なし |
| `\` で終わる quoted-string (escape 未完了) | accept / content_type / expect すべて `UnterminatedQuote` | issue 0061 で 3 モジュール統一 |
| 中間に CTL (`"\rabc"`) | accept: `InvalidParameter`、content_type: `InvalidParameter`、expect: `InvalidValue` | 文字種エラーが優先 |
| 末尾に CR/LF (`"foo\r\n`) | 上流の `trim()` が CR/LF を消費するため `parse_quoted_string` には届かず、`UnterminatedQuote` で reject される | trim 仕様に依存 |
| DQUOTE (%x22) の裸の出現 | qdtext 経路で到達不能 (DQUOTE は終端として処理済み)。quoted-pair `\"` としてのみ出現可能 | 変更なし |
| `"` の後ろにゴミ | accept / expect の `remaining.trim().is_empty()` で既にチェック | 変更なし |

## CHANGES.md

`## develop` に `[CHANGE]` 2 件 + `[FIX]` 1 件を追記する。`[CHANGE]` は `AcceptError` / `ExpectError` への `UnterminatedQuote` バリアント追加 (破壊的) と、`Accept` / `Content-Type` の Display で空値を `name=""` で出力するようにした既存挙動変更。`[FIX]` は本 issue の主目的である qdtext / quoted-pair 文字種検証の追加。

## ブランチ名

`feature/fix-unify-quoted-string-validation`

## 受け入れ基準

- [ ] 3 モジュールの `parse_quoted_string` で `is_qdtext_char` / `is_quoted_pair_char` による文字種検証が追加されている
- [ ] `make fmt && make clippy && make check && make test` が pass
- [ ] `cargo test -p shiguredo_http11 --test test_accept` / `test_content_type` / `test_expect` が pass
- [ ] `cargo test -p pbt --test prop_accept` / `prop_content_type` / `prop_expect` が pass
- [ ] CR / LF / NUL を含む quoted-string / quoted-pair が reject される (3 モジュールそれぞれ)
- [ ] 他の CTL (0x01-0x08, 0x0B-0x0C, 0x0E-0x1F, 0x7F) も reject される
- [ ] obs-text (U+0080 以上) を含む quoted-string は引き続き受理される
- [ ] 空 quoted-string `""` が 3 モジュールすべてで引き続き受理される
- [ ] 通常の VCHAR / HTAB / SP は引き続き受理される (リグレッション防止)
- [ ] `cargo +nightly fuzz run fuzz_accept -- -max_total_time=60` / `fuzz_content_type` / `fuzz_expect` で新規 crash が出ない
- [ ] `CHANGES.md` に `[FIX]` エントリが追記されている

## 解決方法

### 実装

- `src/validate.rs` に `pub(crate) enum QuotedStringError { InvalidQdtext, InvalidQuotedPair, Unterminated }` と `pub(crate) fn parse_quoted_string(input: &str) -> Result<(String, &str), QuotedStringError>` を新設し、3 モジュール共通の検証ロジックに集約する。
- `src/accept.rs` / `src/content_type.rs` / `src/expect.rs` の旧 `parse_quoted_string` を削除し、`validate::parse_quoted_string` の戻り値を `From<QuotedStringError>` 経由で各モジュールのエラー型へ変換する。
- `AcceptError::UnterminatedQuote` / `ExpectError::UnterminatedQuote` を新規追加し、`ContentTypeError::UnterminatedQuote` と粒度を揃える (破壊的変更、CHANGES.md に `[CHANGE]` エントリ)。
- `accept.rs` / `content_type.rs` の `needs_quoting` を空文字列で `true` を返すよう修正し、`Display` ラウンドトリップ破綻 (空値が `name=` で出力されて再パース不可) を解消する (破壊的変更、CHANGES.md に `[CHANGE]` エントリ)。
- obs-text (U+0080..=U+10FFFF) は opaque data として引き続き受理される (`is_qdtext_char` / `is_quoted_pair_char` の Unicode scalar 拡張解釈、issue 0059)。

### テスト

- 単体テスト (`tests/test_accept.rs` / `tests/test_content_type.rs` / `tests/test_expect.rs`):
  - `tests/helpers/quoted_string.rs` に CTL 集合と obs-text 境界値を共通化。
  - `*_quoted_string_rejects_ctl`: HTAB を除く全 ASCII CTL + DEL を 1 ループで網羅し、qdtext / quoted-pair 両経路で reject されることを確認。中間 CTL (`"\rabc"`) も検証。
  - `*_quoted_string_accepts_obs_text`: obs-text 境界値 (U+0080 / U+00FF / U+0100 / U+1234 / U+D7FF / U+E000 / U+10FFFF) を含む quoted-string が受理される。
  - `test_*_empty_quoted_string` / `test_content_type_empty_quoted_value_roundtrip`: 空 quoted-string `""` の Display ラウンドトリップが破綻しない。
  - `test_*_unterminated_quote`: 終端引用符欠落で `UnterminatedQuote` が返る。
- PBT (`pbt/tests/prop_accept.rs` / `prop_content_type.rs` / `prop_expect.rs`):
  - `pbt/src/lib.rs` に `qdtext_char()` / `qdtext_value(range)` を共通化し、`prop_content_disposition.rs` も同経路に置換。
  - `prop_*_quoted_obs_text_roundtrip`: obs-text を含む quoted parameter の parse / Display / reparse 一貫性と、Display 出力に value がそのまま埋め込まれることを直接 assert する。

### Fuzzing

- `cargo +nightly fuzz run fuzz_accept / fuzz_content_type / fuzz_expect -- -max_total_time=60` を実行し、いずれも新規 crash なし。
