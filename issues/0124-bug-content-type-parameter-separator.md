# Content-Type パラメータ区切りがセミコロン以外も受理される

- Priority: Medium
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-content-type-parameter-separator
- Polished: {YYYY-MM-DD}

## 目的

`Content-Type` ヘッダーの `type/subtype` と `parameter` 間、および parameter 間の区切りを厳密にセミコロン `;` のみに制限する。

## 優先度根拠

Medium とする。RFC 9110 Section 8.3 では `Content-Type = media-type`、`media-type = type "/" subtype *( OWS ";" OWS parameter )` と規定されている。現状は区切り文字の検証が緩く、カンマ等で parameter が誤認識される可能性がある。

## 現状

- `src/content_type.rs` : `ContentType` 型とパーサー。
- 区切り文字が適切に検証されていない箇所がある可能性がある。

## 設計方針

1. `type/subtype` 以降の parameter 区切りを `;` のみにする。
2. parameter 名と値の間の `=` は維持する。
3. OWS は許容する。

## 完了条件

- `text/html, charset=utf-8` 等が拒否されること。
- `text/html; charset=utf-8` は受理されること。
- テストが追加されること。

## 解決方法

- `src/content_type.rs` のパーサーで parameter 区切りを厳密に検証する。
- エラーを追加または既存エラーを使用する。
- テストを追加する。
