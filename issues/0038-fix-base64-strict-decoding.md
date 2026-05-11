# 0038: Base64 デコードを RFC 4648 Section 3.5 strict に厳格化する

Created: 2026-05-12
Model: Opus 4.7

## 概要

`src/base64.rs::decode` は以下の点で RFC 4648 (特に Section 3.5 / 3.3) のストリクトデコーディングを満たしていない:

1. **入力長 4 の倍数チェックなし**: パディング `=` を含めた入力長が 4 の倍数でなくても受理する
2. **末尾 `=` 個数の整合性チェックなし**: `trim_end_matches('=')` で `=` 数を無視するため、`A===` のような不正パディングを受理
3. **末尾の不完全 6 bit 群を黙って捨てる**: `bits >= 1` の残余 bit を捨てているため、`A` (1 文字) や `AB` (2 文字、パディングなし) のような不正入力で空 / 1 バイトを「成功」として返す
4. **末尾の非ゼロ余剰 bit を許容**: canonical でない base64 (`Zg=` 等の short padding や末尾余剰 bit を持つもの) を受理する

これにより `BasicAuth::parse` 経由で同一の credentials に対して複数の base64 表現が成立し、credential canonicalization が破られる。`Digest`/`Basic` での replay 攻撃の補助となる経路。

## 根拠

### RFC 4648

- Section 3.3「Bits less than 8 ... MUST be zero」
- Section 3.5「Implementations MUST reject the encoded data if it contains characters outside the base alphabet」
- パディング前のデータ文字数:
  - mod 4 == 0: パディング `=` 0 個
  - mod 4 == 2: パディング `=` 2 個
  - mod 4 == 3: パディング `=` 1 個
  - mod 4 == 1: 不可能 (`=` 3 個になり仕様違反)

### 攻撃シナリオ

1. 攻撃者が同一の credential `(user, password)` に対して、複数の base64 表現を生成する
2. 例: `Zg==` (canonical) と `Zg===` (パディング過多) が両方「`f` を表す」として受理されると、認証ログ追跡 / replay 防止 / canonicalization に基づくセキュリティ機構が壊れる
3. token68 検証 (`is_token68`) はパディングの個数を制限しないため、複数表現が認証ヘッダーで成立する

### コードベース内の不整合

- `auth.rs::is_token68` は `trim_end_matches('=')` で `=` 個数を無視する
- 同様に `base64::decode` も `trim_end_matches('=')` で個数を無視する
- 双方で不整合なく canonical 化されていない

## 対応方針

### `src/base64.rs`

- `Base64Error::InvalidPadding` バリアントを追加 (`InvalidCharacter` だけでは識別できないケース)
- `decode` を以下に書き換え:
  1. 入力から空白 (` ` / `\t` / `\n` / `\r`) を除去した中間バッファを作る
  2. 末尾の `=` 数を数える (`pad_count`)
  3. `pad_count > 2` の場合は `InvalidPadding`
  4. パディング含めた入力長が 4 の倍数でない場合は `InvalidPadding`
  5. データ部分の文字数 mod 4 と `pad_count` の整合性チェック:
     - `pad_count == 1` なら data 文字数 mod 4 == 3
     - `pad_count == 2` なら data 文字数 mod 4 == 2
     - `pad_count == 0` なら data 文字数 mod 4 == 0
  6. データ部分を 6 bit ずつ復号
  7. 末尾の余剰 bit (`buf != 0`) は `InvalidPadding` で reject (RFC 4648 §3.3 MUST zero)

### `src/auth.rs::is_token68`

本 issue のスコープ外。`auth.rs` 側の token68 検証は base64 文字集合とパディング個数の限定的検査であり、最終的な base64 デコード時のストリクト検証で credential canonicalization が達成される。

### テスト

- `src/base64.rs` inline test:
  - `decode("A")` (1 文字、パディングなし) → `InvalidPadding`
  - `decode("Zg")` (2 文字、パディングなし) → `InvalidPadding`
  - `decode("Zg===")` (パディング 3 個) → `InvalidPadding`
  - `decode("Zg=")` (パディング 1 個、データ 2 文字) → `InvalidPadding` (mod 4 不整合)
  - `decode("Zh==")` の末尾余剰 bit が 0 でないケース (`buf != 0`) → `InvalidPadding`
- 既存テスト (canonical 入力) は全件継続パス

### CHANGES.md

`## develop` のメインに `[FIX]` として追記する。受理範囲が縮小するため明記する。

### 破壊的変更

- non-canonical base64 を以前受理していたケースは reject される
- `Base64Error::InvalidPadding` バリアント追加 (`pub(crate)` enum のため外部 API には露出しない、`BasicAuth::parse` 等は引き続き `AuthError::Base64DecodeError` で潰す)
- canary リリース中なので破壊的変更は許容範囲
