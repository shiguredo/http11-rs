# Accept パラメータ名の token 検証が不足している

- Priority: Medium
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-accept-parameter-token-validation
- Polished: {YYYY-MM-DD}

## 目的

`Accept` / `AcceptCharset` / `AcceptEncoding` / `AcceptLanguage` 等の `q` 以外のパラメータ名が `token` として検証されるようにする。

## 優先度根拠

Medium とする。RFC 9110 Section 12.5 の `accept-params` / `accept-ext` はパラメータ名を `token` とする。現状は `q` 値の検証は行われているが、拡張パラメータ名の token 検証が行われていない可能性がある。

## 現状

- `src/accept.rs` : `parse_parameters` 関数等でパラメータを解析。
- 拡張パラメータ名が `=` や空白、区切り文字を含む場合に受理されてしまう可能性がある。

## 設計方針

1. パラメータ名を `validate_token` で検証する。
2. `q` 値の検証は既存のまま維持する。
3. エラーメッセージを改善する。

## 完了条件

- パラメータ名が `token` 規則に違反する場合に `InvalidAccept` エラーが返されること。
- 正当な拡張パラメータは引き続き受理されること。
- テストが追加されること。

## 解決方法

- `src/accept.rs` のパラメータ解析箇所でパラメータ名を `validate_token` に通す。
- エラーを適切に返す。
- テストを追加する。
