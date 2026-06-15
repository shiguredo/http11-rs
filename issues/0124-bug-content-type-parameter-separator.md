# Content-Type パラメータ区切りをセミコロンのみに厳密化する

- Priority: Medium
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-content-type-parameter-separator
- Polished: 2026-06-16

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
  - `text/html, charset=utf-8` 等のカンマ区切りは区切り文字の検証が明示的でないため、本 issue で明示拒否する (現状は下流の token 検証で偶然拒否されている)。

## 設計方針

1. parameter を 1 つ消費した後の **次区切りの検証** で、区切り文字を `;` のみに限定する。`type/subtype` 直後の最初の `;` 出現は既存の `split_at_semicolon` (`L221`) が扱い、本 issue は **parameter 直後の次区切り検証時** のみ厳密化対象とする。
2. parameter 名と値の間の `=` は既存の解釈を維持する (本 issue では `=` 周りの OWS 取り扱いは変更しない)。
3. `OWS ";" OWS` 形式は許容する (RFC 9110 Section 5.6.6 ABNF `parameters = *( OWS ";" OWS [ parameter ] )` に従う)。
4. 空の parameter セグメント (連続するセミコロン `;;`) は RFC 準拠として許容する (ABNF の `[ parameter ]` が optional のため)。
5. 区切り文字違反時は既存の `ContentTypeError::InvalidParameter` を返す。

### 実装 Skeleton

`parse_parameters` (`L254-`) のループ内 `remaining` 処理を以下に変更する。

```rust
// 1 つの parameter をパース後の残り入力を以下のパターンで処理する
let remaining = trim_ows(remaining);
if remaining.is_empty() {
    break;
}
// `;` で始まらなければ区切り違反として reject
let after_semi = remaining
    .strip_prefix(';')
    .ok_or(ContentTypeError::InvalidParameter)?;
rest = trim_ows(after_semi);
// 空 parameter セグメント (;;) は次の loop iteration で trim_ows → strip_prefix(';') が連鎖する
```

既存の `L260` の `rest = trim_ows(rest.trim_start_matches(';'))` (ループ先頭) は、最初の `split_at_semicolon` 後の `;` 前置に対する処理として残すか、設計方針 1 の「最初の `;` は `split_at_semicolon` で扱う」前提で削除する。`split_at_semicolon` 後の rest は parameter 部分のみを含むため、ループ先頭での `trim_start_matches(';')` は冗長で削除可能。

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
- `CHANGES.md` の `## develop` セクションに `[FIX]` エントリを `[ADD]` の下、`### misc` の上に追加すること (本体ライブラリのバグ修正)。例: `[FIX] Content-Type ヘッダーの parameter 区切りをセミコロンのみに厳密化する`。

## 解決方法

- `src/content_type.rs` の `parse_parameters` で、parameter を 1 つパースした後の残り入力が空でない場合、先頭の OWS を除去した直後が `;` で始まることを明示的に検証する。`;` で始まらなければ `InvalidParameter` を返す。
- parameter 間の区切り文字を厳密に検証するテストを `tests/test_content_type.rs` に追加する。
- `CHANGES.md` を更新する。
