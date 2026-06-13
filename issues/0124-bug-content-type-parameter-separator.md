# Content-Type パラメータ区切りをセミコロンのみに厳密化する

- Priority: Medium
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-content-type-parameter-separator
- Polished: 2026-06-13

## 目的

`Content-Type` ヘッダーの `type/subtype` と parameter 間、および parameter 間の区切りを RFC 9110 に従いセミコロン `;` のみに厳密化する。

## 優先度根拠

Medium とする。RFC 9110 Section 8.3 では `Content-Type = media-type`、Section 8.3.1 では `media-type = type "/" subtype parameters`、Section 5.6.6 では `parameters = *( OWS ";" OWS [ parameter ] )` と規定されている。現状は parameter 間の区切りがセミコロン以外（空白や HTAB のみ）でも受理されてしまい、非準拠なヘッダーを誤って解釈する。

## 現状

- `src/content_type.rs` : `ContentType` 型とパーサー。
- `split_at_semicolon` (line 221) は最初の `;` で `type/subtype` 部分と parameter 部分を分断する。
- `parse_parameters` (line 254) は parameter を 1 つパースした後、残り入力から先頭の `;` を `trim_start_matches(';')` で削除するだけである。セミコロンが存在しなかった場合（空白や HTAB のみで次の parameter が続く場合）も誤って受理してしまう。
- 具体例:
  - `text/html; charset=utf-8 boundary=something` が空白区切りで受理される（不正）。
  - `text/html; charset=utf-8\tboundary=something` が HTAB 区切りで受理される（不正）。
  - `text/html; charset=utf-8 boundary=something; foo=bar` が受理される（不正）。
  - `text/html, charset=utf-8` 等のカンマ区切りは現状の下流の token 検証で偶然拒否されるが、区切り文字の検証が明示的ではない。

## 設計方針

1. `type/subtype` 以降の parameter 区切りを `;` のみにする。
2. parameter 名と値の間の `=` は既存の解釈を維持する（本 issue では `=` 周りの OWS 取り扱いは変更しない）。
3. `OWS ";" OWS` 形式は許容する。
4. 空の parameter セグメント（連続するセミコロン）は RFC 準拠として許容する。
5. 区切り文字違反時は既存の `ContentTypeError::InvalidParameter` を返す。

## 完了条件

- `text/html; charset=utf-8` は受理されること。
- `text/html; charset=utf-8; boundary=something` は受理されること。
- `text/html; charset=utf-8 ; boundary=something` は受理されること。
- `text/html;; charset=utf-8` は受理されること（空 parameter セグメント許容）。
- `text/html; charset=utf-8 boundary=something` は拒否されること。
- `text/html; charset=utf-8\tboundary=something` は拒否されること。
- `text/html; charset=utf-8, boundary=something` は拒否されること。
- `text/html, charset=utf-8` は拒否されること。
- `tests/test_content_type.rs` に上記の受理・拒否テストが追加されること。
- `CHANGES.md` に `[FIX]` エントリが追加されること。

## 解決方法

- `src/content_type.rs` の `parse_parameters` で、parameter を 1 つパースした後の残り入力が空でない場合、先頭の OWS を除去した直後が `;` で始まることを明示的に検証する。`;` で始まらなければ `InvalidParameter` を返す。
- parameter 間の区切り文字を厳密に検証するテストを `tests/test_content_type.rs` に追加する。
- `CHANGES.md` を更新する。
