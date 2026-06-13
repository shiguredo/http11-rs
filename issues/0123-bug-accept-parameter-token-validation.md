# Accept メディアレンジのパラメータ名が token として検証されていない

- Priority: Medium
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-accept-parameter-token-validation
- Polished: 2026-06-13

## 目的

RFC 9110 Section 5.6.6 / Section 12.5.1 に従い、`Accept` ヘッダーのメディアレンジに付随するパラメータ名を `token` として検証する。現状は `q` 値の検証は行われているが、メディアタイプパラメータ名の `token` 検証が行われていないため、空白や区切り文字、空文字列を含む不正なパラメータ名を受理してしまう。

## 優先度根拠

Medium とする。RFC 9110 Section 5.6.6 では `parameter-name = token` と明確に規定されている。受信側が不正なパラメータ名を黙って受理すると、後続のコンテンツネゴシエーションやメディアタイプ解釈で予期しない動作を招く可能性がある。ただし、プロトコルフレーミングを破壊するような重大な脆弱性ではないため Medium とする。

## 現状

- `src/accept.rs` の `parse_media_range_item` 関数（現在の実装ではおおむね行 422-457、パラメータ処理ループは 431-449）で、パラメータを `=` で分割した後、パラメータ名に対して `token` 検証を行っていない。
- その結果、以下のような入力が `Ok` として受理されてしまう。
  - `text/html; foo bar=value`（空白を含むパラメータ名）
  - `text/html; foo@bar=value`（区切り文字 `@` を含むパラメータ名）
  - `text/html; =value`（空のパラメータ名）
- 一方で、`AcceptCharset` / `AcceptEncoding` / `AcceptLanguage` は `parse_weighted_tokens` 関数（現在の実装ではおおむね行 497-557）で `q` 以外のパラメータを既に `AcceptError::InvalidParameter` で拒否しており、これらは RFC 9110 準拠である。

## 設計方針

1. `parse_media_range_item` 内でパラメータ名を `trim_ows` した直後、小文字化する前の name に対して `is_valid_token` で検証する。`is_valid_token` は空文字列も拒否するため、空のパラメータ名も同時に排除される。
2. `q` パラメータの特別扱いと `q` 値の検証は既存のまま維持する。
3. `AcceptCharset` / `AcceptEncoding` / `AcceptLanguage` は `q` 以外のパラメータを拒否する既存の動作を維持する。これらのフィールドでは RFC 9110 上で `weight` 以外のパラメータは許容されない。
4. パラメータ名の `token` 違反は `AcceptError::InvalidToken` を返す。これは `parse_param_value` 内でパラメータ値の token 違反に対して既に返しているエラー種別と統一する。
5. エラーメッセージは既存の `InvalidToken` 表示（`"invalid token"`）を利用する。必要に応じて、パラメータ名が不正であることが分かる補足情報を検討する。

## 完了条件

- `Accept::parse` に対し、以下の不正なパラメータ名を含む入力が `Err(AcceptError::InvalidToken)` を返すこと。
  - 空白を含むパラメータ名（例: `text/html; foo bar=value`）
  - 区切り文字を含むパラメータ名（例: `text/html; foo@bar=value`、`text/html; foo[bar]=value`）
  - 空のパラメータ名（例: `text/html; =value`）
  - `token = 1*tchar` なので空文字列は本来許容されず、`is_valid_token` も空文字列を拒否する。
- 正当なパラメータ名は引き続き受理されること。
  - `text/html; charset=utf-8`
  - `text/html; level=1`
  - `text/html; format=flowed`
  - `text/html; charset=value; q=0.5`
- `AcceptCharset::parse` / `AcceptEncoding::parse` / `AcceptLanguage::parse` は `q` 以外のパラメータを引き続き `Err(AcceptError::InvalidParameter)` で拒否すること。
- `tests/test_accept.rs` に不正パラメータ名に対するテストケースを追加すること。
- `CHANGES.md` の `## develop` セクションに以下の `[FIX]` エントリを追加すること。
  - `[FIX] Accept ヘッダーのメディアレンジパラメータ名が token 規則に違反していても受理されていた問題を修正する`

## 解決方法

- `src/accept.rs` の `parse_media_range_item` 関数で、パラメータ名を `trim_ows` 後・小文字化する前に `is_valid_token` で検証し、偽なら `AcceptError::InvalidToken` を返す。
- `AcceptCharset` / `AcceptEncoding` / `AcceptLanguage` の実装は変更しない。
- `tests/test_accept.rs` に不正パラメータ名と正当パラメータ名の両方を網羅したテストを追加する。
- `CHANGES.md` を更新する。

## 参考: RFC 文面

- RFC 9110 Section 5.6.6
  > parameter-name = token
  > parameter-value = ( token / quoted-string )
- RFC 9110 Section 5.6.2
  > token = 1*tchar
  > tchar = "!" / "#" / "$" / "%" / "&" / "'" / "*" / "+" / "-" / "." / "^" / "_" / "`" / "|" / "~" / DIGIT / ALPHA
- RFC 9110 Section 12.5.1
  > Accept = #( media-range [ weight ] )
  > media-range = ( "*/*" / ( type "/" "*" ) / ( type "/" subtype ) ) parameters
  > Each media-range might be followed by optional applicable media type parameters (e.g., charset), followed by an optional "q" parameter for indicating a relative weight (Section 12.4.2).
  > Previous specifications allowed additional extension parameters to appear after the weight parameter. The accept extension grammar (accept-params, accept-ext) has been removed because it had a complicated definition, was not being used in practice, and is more easily deployed through new header fields.
