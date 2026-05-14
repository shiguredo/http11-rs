# 0061 fix accept / content_type / expect の parse_quoted_string に qdtext / quoted-pair 文字種検証を追加する

Created: 2026-05-14
Model: deepseek-v4-pro

## 概要

`src/accept.rs`、`src/content_type.rs`、`src/expect.rs` の 3 モジュールに存在する `parse_quoted_string` 関数が、引用符内の全文字を無条件に受理しており、qtext / quoted-pair の文字種検証を一切行っていない。このため CR (0x0D)、LF (0x0A)、NUL (0x00) を含む任意の制御文字が quoted-string 値として保存される。

RFC 9110 Section 5.5 は「CR, LF, or NUL within a field value MUST either reject the message or replace each of those characters with SP」と規定しており、MUST 要件違反である。

一方で `src/auth.rs` と `src/content_disposition.rs` の quoted-string パーサーは `validate.rs` の `is_qdtext_char` / `is_quoted_pair_char` を使って正しく検証を行っている。3 モジュールにも同様の検証を追加し、可能であれば共通の `pub(crate) fn parse_quoted_string` に抽出する。

## 再現手順

1. `Accept::parse("text/html; q=\"\r\n\"")` が制御文字 `\r\n` を含む quoted-string を正常に受理する
2. 受理された値が上位アプリで再生成されると HTTP Response Splitting (CWE-113) の経路となる

## 対象ファイル

- `src/accept.rs:556-573` (`parse_quoted_string`)
- `src/content_type.rs:275-293` (`parse_quoted_string`)
- `src/expect.rs:185-203` (`parse_quoted_string`)

## 推奨対応

- `validate.rs` の `is_qdtext_char` / `is_quoted_pair_char` を使って文字種検証を追加する
- `escaped` 時と非 `escaped` 時の両方で検証を行う
- 検証失敗時は既存のエラー型 (`AcceptError::InvalidParameter`、`ContentTypeError::InvalidParameter`、`ExpectError::InvalidValue`) を返す
- 理想的には `validate.rs` に `pub(crate) fn parse_quoted_string(input: &str) -> Result<(String, &str), &str>` を抽出して共通化する
