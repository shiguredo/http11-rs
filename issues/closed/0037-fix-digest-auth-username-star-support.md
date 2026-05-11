# 0037: DigestAuth で username* (RFC 7616 §3.4) をサポートする

Created: 2026-05-12
Completed: 2026-05-12
Model: Opus 4.7

## 概要

`src/auth.rs::DigestAuth::parse` は必須パラメータとして `["username", "realm", "nonce", "uri", "response"]` を要求している。しかし RFC 7616 §3.4 では:

- `username` (ASCII) または `username*` (RFC 5987 ext-value、UTF-8) の **どちらか一方** が必須
- 両方同時に送られた場合は MUST NOT (= reject すべき)

現状の実装では `username` がないと無条件に `MissingParameter` で reject するため、UTF-8 ユーザー名を `username*=` で送信するクライアントを全部弾く。これは相互運用性の致命的な欠陥。

## 根拠

### RFC 7616 §3.4

```
   If the username contains characters not allowed inside the ABNF
   quoted-string production, the user's name MUST be sent using the
   "username*" parameter ... using the value encoding defined in
   Section 3.2.1 of [RFC5987].
```

```
   If the "userhash" parameter value is "true" ... then ... the digest's
   "username" parameter is used to convey the hash.

   ... the client MUST send the "username*" parameter instead of the
   "username" parameter as defined in Section 3.4.
```

つまり `username` か `username*` のどちらか一方が必須で、`username*` は UTF-8 ユーザー名のために RFC 5987 ext-value 形式で送る。

### 影響範囲

- UTF-8 ユーザー名 (日本語ユーザー名等) を扱う Digest クライアントは本実装と相互運用できない
- 攻撃シナリオではないが、HTTP Authorization の仕様準拠として致命的

## 対応方針

### `src/auth.rs`

- `AuthError` に `ConflictingUsernameField` (両方同時送信時) を追加する
- `DigestAuth::parse` の必須パラメータチェックを以下に変更:
  - `realm`, `nonce`, `uri`, `response` は引き続き必須
  - `username` と `username*` は XOR (どちらか一方が必須、両方は reject)
- `DigestAuth::username()` を以下に変更:
  - `username` パラメータがあればその値を返す (ASCII)
  - `username*` パラメータがあれば RFC 5987 ext-value デコードして UTF-8 文字列として返す
- RFC 5987 ext-value のデコード関数 `parse_username_ext_value` を auth.rs 内に新設する (`content_disposition.rs::parse_ext_value` と同等のロジック)

### `src/content_disposition.rs::parse_ext_value`

将来的に共通化する余地があるが、本 issue では auth.rs 側に独立実装を置く。content_disposition と auth で重複する `parse_ext_value` は後続 issue で `validate.rs` or `ext_value.rs` モジュールに集約することを検討。

### テスト

- `tests/test_auth.rs`:
  - `Digest username*="UTF-8''%E3%83%A6%E3%83%BC%E3%82%B6", ..."` がパース成功し `username()` が「ユーザ」を返す
  - `username` と `username*` 両方を含むものは `ConflictingUsernameField` で reject
  - `username` のみは引き続き parse 成功
  - `username*` も `username` もない場合は `MissingParameter` で reject
  - `username*` の charset が UTF-8 以外 (例: `ISO-8859-1`) も RFC 5987 §3.2.1 通り decode 試行する (シンプル実装としては UTF-8 のみサポートでも OK、その場合は `InvalidCharset` エラー)

### CHANGES.md

`## develop` のメインに `[FIX]` として追記する (相互運用性バグ修正)。

### 破壊的変更

- 新エラー `AuthError::ConflictingUsernameField` / `AuthError::InvalidUsernameExtValue` の追加 (enum 拡張、`#[non_exhaustive]` 未付与のため API 破壊)
- `DigestAuth::username()` は ASCII のみを返すよう据え置き、UTF-8 用に `DigestAuth::username_decoded()` を新設
- canary 中の破壊的変更として CHANGES.md に `[FIX]` で記録

## 解決方法

- `src/auth.rs::AuthError` に 2 バリアント追加: `ConflictingUsernameField` / `InvalidUsernameExtValue`
- `src/auth.rs::AuthError::Display` 実装も対応追記
- `src/auth.rs::DigestAuth::parse` を以下に変更:
  - `username` と `username*` の XOR 判定を導入 (RFC 7616 §3.4)
  - `username*` がある場合は parse 時に `decode_username_ext_value` で事前検証 (`username_decoded()` を infallible にするため)
  - `realm` / `nonce` / `uri` / `response` の必須チェックは継続
- `DigestAuth::username()` を「`username` パラメータがある場合のみその値を返す」と doc 修正 (ASCII)
- `DigestAuth::username_decoded()` を新設: `username` or `username*` のどちらかから UTF-8 ユーザー名を返す
- `src/auth.rs` に以下のヘルパー関数を追加:
  - `decode_username_ext_value(input) -> Result<String, AuthError>`: RFC 5987 ext-value (`charset 'language' value`) を decode、charset は UTF-8 限定、percent-decode + attr-char 検証
  - `is_attr_char(b) -> bool`: RFC 5987 §3.2.1 の attr-char
- テスト追加 (`tests/test_auth.rs`):
  - `test_digest_auth_accepts_username_star_with_utf8`: `ユーザ` を UTF-8 percent-encoded で受理
  - `test_digest_auth_accepts_username_ascii_only`: ASCII の `username` のみは引き続き受理
  - `test_digest_auth_rejects_both_username_and_star`: 両方同時で `ConflictingUsernameField`
  - `test_digest_auth_rejects_missing_both_username_fields`: どちらもない場合は `MissingParameter`
  - `test_digest_auth_rejects_username_star_non_utf8_charset`: UTF-8 以外の charset は `InvalidUsernameExtValue`
  - `test_digest_auth_rejects_username_star_invalid_ext_value`: ext-value 形式不正は `InvalidUsernameExtValue`
  - `test_auth_error_display`: 新バリアント 2 件の Display 表示を確認
- `CHANGES.md` の `## develop` に `[FIX]` エントリを追加した
