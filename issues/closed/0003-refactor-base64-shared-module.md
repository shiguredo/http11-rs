# 0003: Base64 実装の共通モジュール化

Created: 2026-04-28
Completed: 2026-04-28
Model: Opus 4.7 (1M context)

## 概要

`src/auth.rs` と `src/digest_fields.rs` に同一の Base64 エンコード/デコード実装が重複している。共通モジュール `src/base64.rs` に統合する。

## 重複箇所

| 項目 | `src/auth.rs` | `src/digest_fields.rs` |
| ---- | ------------- | ---------------------- |
| `BASE64_ALPHABET` 定数 | L884-L885 | L365-L366 |
| `base64_encode` | L888-L918 | L369-L399 |
| `base64_decode` | L921-L950 | L402-L431 |

`BASE64_ALPHABET` と `base64_encode` は完全一致。`base64_decode` はロジック完全一致で、エラー時の戻り値だけが各モジュール固有のエラー型 (`AuthError::Base64DecodeError` / `DigestFieldsError::InvalidBase64`) になっている。

## 対応が必要な根拠

- 同一ロジックの二重保守は変更時の整合性リスクを生む。RFC 4648 のアルファベットや空白許容ポリシーなどを片方だけ修正するとバグになりやすい。
- `auth.rs` 側にはユニットテスト (`test_base64_encode` / `test_base64_decode`) があるが `digest_fields.rs` 側の同実装には対応するテストがなく、テスト資産も二重化を避けるべき。
- 今後 RFC 7616 (Digest 認証の `qop=auth-int` 用ハッシュ) や他ヘッダーで Base64 が再度必要になった場合、共通モジュールがあれば追加コストがゼロになる。

## 対応方針

- `src/base64.rs` を新規作成し、以下を実装する。
  - `pub(crate) const BASE64_ALPHABET`
  - `pub(crate) fn encode(input: &[u8]) -> alloc::string::String`
  - `pub(crate) fn decode(input: &str) -> Result<alloc::vec::Vec<u8>, Base64Error>`
  - `pub(crate) enum Base64Error { InvalidCharacter }` (中立な独自型)
- `src/lib.rs` に `mod base64;` を追加する (非公開モジュール)。
- `src/auth.rs` から重複実装を削除し、`crate::base64::{encode, decode}` を呼び出す。デコードのエラーは `map_err(|_| AuthError::Base64DecodeError)` で従来のエラー型に変換する (後方互換維持)。
- `src/digest_fields.rs` から重複実装を削除し、同様に `map_err(|_| DigestFieldsError::InvalidBase64)` で変換する。
- `auth.rs` 側のユニットテストは `base64.rs` 側に移動する。

## 後方互換

- 公開 API は変更しない。`AuthError::Base64DecodeError` および `DigestFieldsError::InvalidBase64` は引き続き返る。

## 検証

- `make fmt && make clippy && make check && make test` を通す。
- 既存の `tests/test_auth.rs` / `tests/test_digest_fields.rs` / `pbt/tests/prop_digest_fields.rs` がそのまま緑であることを確認する。

## 解決方法

- `src/base64.rs` を新規作成し、`pub(crate) const BASE64_ALPHABET` / `pub(crate) fn encode` / `pub(crate) fn decode` / `pub(crate) enum Base64Error { InvalidCharacter }` を実装した。
- `src/lib.rs` に `mod base64;` を追加した (非公開モジュール)。
- `src/auth.rs` から重複していた `BASE64_ALPHABET` / `base64_encode` / `base64_decode` を削除し、`base64::encode` / `base64::decode(...).map_err(|_| AuthError::Base64DecodeError)` を呼び出す形に変更した。`auth.rs` 内にあった `test_base64_encode` / `test_base64_decode` は `base64.rs` の `#[cfg(test)] mod tests` に移動した。
- `src/digest_fields.rs` から同じく重複実装を削除し、`base64::encode` / `base64::decode(...).map_err(|_| DigestFieldsError::InvalidBase64)` に置き換えた。シャドウイングを避けるためローカル変数 `base64` は `encoded` にリネームした。
- 公開 API は不変。`AuthError::Base64DecodeError` / `DigestFieldsError::InvalidBase64` は引き続き返る。
- 検証: `make fmt`、`make clippy` (-D warnings)、`make check`、`make test` をすべて通過した。
