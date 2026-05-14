# 0061 fix accept / content_type / expect の parse_quoted_string に qdtext / quoted-pair 文字種検証を追加する

Created: 2026-05-14
Model: deepseek-v4-pro

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

Accept / Content-Type / Expect のパラメータ値は上記定義に従い `quoted-string` 構文を取り得る (`refs/rfc9110.txt:1818-1821`、1826-1849)。

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
3. Expect: `Expect::parse("\"100-continue\r\n\"")` が extended expectation の quoted-string 経路で制御文字を受理する
4. 受理された値が上位アプリで再生成されると HTTP Response Splitting (CWE-113) の経路となる

## 対象ファイル

- `src/accept.rs:556-573` (`parse_quoted_string`) — 呼び出し元: `parse_param_value` (line 540-554)
- `src/content_type.rs:275-293` (`parse_quoted_string`) — 呼び出し元: `parse_parameters` (line 259-265)
- `src/expect.rs:185-203` (`parse_quoted_string`) — 呼び出し元: `parse_value` (line 165-183)

修正対象外:
- `src/accept.rs` と `src/expect.rs` の `split_with_quotes` — 分割が責務であり、文字種検証は `parse_quoted_string` に委譲するため変更不要

## 推奨対応

### 各モジュールの修正

各 `parse_quoted_string` の冒頭に import を追加し、ループ内で文字種検証を行う。

```rust
use crate::validate::{is_qdtext_char, is_quoted_pair_char};
```

- **非 escaped 時**: `is_qdtext_char(c)` が `false` ならエラー
- **escaped 時**: `is_quoted_pair_char(next_c)` が `false` ならエラー

検証失敗時のエラー型:

| モジュール | 文字種不正 | 終端引用符なし |
|---|---|---|
| `accept.rs` | `AcceptError::InvalidParameter` | `AcceptError::InvalidParameter` |
| `content_type.rs` | `ContentTypeError::InvalidParameter` | `ContentTypeError::UnterminatedQuote` (既存維持) |
| `expect.rs` | `ExpectError::InvalidValue` | `ExpectError::InvalidValue` |

`content_type.rs` は従来通り構造エラー（終端引用符なし）を `UnterminatedQuote`、文字種エラーを `InvalidParameter` と区別する。検証順序により `"\r\n` (文字種不正 + 終端引用符なし) の入力は修正前は `UnterminatedQuote`、修正後は `InvalidParameter` を返す（変更あり）。

共通抽出は戻り値型・エラー型・可視性制約の差異の吸収コストが本 issue のスコープを超えるため見送る。

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

- `pbt/tests/prop_expect.rs`: 既存の `quoted_string_char()` strategy は有効な qdtext のみ (ASCII 範囲) を生成し obs-text を含まない。修正後もラウンドトリップは既存取材で成立する。obs-text 経路の PBT カバレッジは後続 issue または本 issue のフォローアップで別途拡張する（注記として残す）。
- `pbt/tests/prop_content_type.rs`: `prop_content_disposition.rs:19-32` の `qdtext_char()` を参考に、obs-text を含む `qdtext_char` strategy を新設し、quoted parameter 経路でラウンドトリッププロパティを追加する。
- `pbt/tests/prop_accept.rs`: `prop_content_disposition.rs` の `qdtext_char()` を参考に同様の strategy を新設し、`prop_accept_with_params` と同形式の新規テスト関数を追加して quoted parameter 経路のラウンドトリップを検証する。

### Fuzzing

- 既存の `fuzz_accept.rs`、`fuzz_content_type.rs`、`fuzz_expect.rs` は任意 UTF-8 入力でパースを試行する。修正後は reject パスが増えるのみで panic 安全性上の変化はない。変更不要。
- 3 ターゲットとも `cargo +nightly fuzz run <target> -- -max_total_time=60` で新規 crash が無いことを確認する。特に `fuzz_content_type.rs` は Display ラウンドトリップ assertion (line 36-37) が stricter validation 後も破綻しないことを確認する。

## エッジケース

| ケース | 修正後挙動 | 備考 |
|---|---|---|
| 空 quoted-string `""` | 即座に `("", rest)` を返す | 既存通り |
| obs-text (U+0080..=U+10FFFF) | `is_qdtext_char` / `is_quoted_pair_char` が受理 | opaque 保持、変更不要 |
| `\` で終わる quoted-string (escape 未完了) | accept/expect: `InvalidParameter` / `InvalidValue`。content_type: `UnterminatedQuote` | エラー型の差異は既存通り |
| `"foo\r\n` (文字種不正 + 終端引用符なし) | accept: `InvalidParameter`。content_type: `InvalidParameter` (修正前は `UnterminatedQuote`)。expect: `InvalidValue` | content_type の変更あり |
| DQUOTE (%x22) の裸の出現 | qdtext 経路で到達不能 (DQUOTE は終端として処理済み)。quoted-pair `\"` としてのみ出現可能 | 変更不要 |
| `"` の後ろにゴミ | accept/expect の `remaining.trim().is_empty()` で既にチェック | 変更不要 |

## CHANGES.md

`## develop` に以下を追記する:

```
- [FIX] `Accept` / `Content-Type` / `Expect` の quoted-string パースに qdtext / quoted-pair 文字種検証を追加する
  - 旧実装は RFC 9110 Section 5.5 の CR/LF/NUL MUST 要件に違反し、任意の制御文字を無条件に受理していた
  - `validate.rs` の `is_qdtext_char` / `is_quoted_pair_char` (issue 0036 / 0059 で導入済) を 3 モジュールに追加し、RFC 9110 Section 5.6.4 ABNF 準拠の検証を実施する
  - obs-text (U+0080 以上) は opaque data として引き続き受理する (RFC 9110 Section 5.5、issue 0059 方針)
  - @voluntas
```

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
