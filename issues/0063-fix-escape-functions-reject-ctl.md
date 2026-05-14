# 0063 fix escape 関数群で CR / LF / NUL を reject する

Created: 2026-05-14
Model: deepseek-v4-pro

## 概要

以下の escape 関数がエスケープ対象を `"` と `\` のみに限定しており、CR / LF / NUL を raw のまま出力する:

- `src/content_disposition.rs:526-534` (`escape_quoted_string`)
- `src/auth.rs:928-930` (`escape_quotes`)
- `src/content_type.rs:335-337` (`escape_quotes`)
- `src/accept.rs:658-660` (`escape_quotes`)
- `src/expect.rs:250-252` (`escape_quotes`)

RFC 9110 Section 5.5 は「A sender MUST NOT generate a bare CR or LF」および「sender MUST NOT generate NUL in field values」と規定している。これらの escape 関数を経由して CR / LF / NUL を含む文字列を quoted-string や field-value として生成すると、HTTP Response Splitting (CWE-113) や Header Injection の経路となる。

例: `ContentDisposition::with_filename("file\r\nInjected-Header: evil\r\n")` が生成するヘッダー値は `attachment; filename="file\r\nInjected-Header: evil\r\n"` となり、`Content-Disposition` ヘッダー内で Response Splitting が成立する。

## 再現手順

1. `ContentDisposition::with_filename("file\r\nInjected-Header: evil\r\n").to_header_value()` を呼ぶ
2. `\r\n` がエスケープされずに raw のまま出力される
3. これを下流に送出すると Header Injection が成立する

## 対象ファイル

- `src/content_disposition.rs:526-534`
- `src/auth.rs:928-930`
- `src/content_type.rs:335-337`
- `src/accept.rs:658-660`
- `src/expect.rs:250-252`

## 推奨対応

各 escape 関数で CR (`\r`)、LF (`\n`)、NUL (`\0`) を検出した場合に拒否する。
理想的には `validate.rs` に `pub(crate) fn escape_quoted_string(input: &str) -> Result<String, ...>` を用意して共通化する。
