# Digest 認証の to_header_value が RFC 7616 の quoting 規則に違反

- Priority: High
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-digest-auth-rfc7616-quoting
- Polished: 2026-06-13

## 目的

`DigestAuth::to_header_value` / `DigestChallenge::to_header_value` が RFC 7616 Section 3.3 / 3.4 で規定されたパラメータの quoting 規則に従っていない。送信側で必ず quoted-string 形式を出力すべきパラメータと、必ず token 形式を出力すべきパラメータを正しく区別する。

## 優先度根拠

High とする。RFC 7616 Section 3.3 では以下のように規定している。

> For historical reasons, a sender MUST only generate the quoted string syntax values for the following parameters: realm, domain, nonce, opaque, and qop.
> For historical reasons, a sender MUST NOT generate the quoted string syntax values for the following parameters: stale and algorithm.

RFC 7616 Section 3.4 では以下のように規定している。

> For historical reasons, a sender MUST only generate the quoted string syntax for the following parameters: username, realm, nonce, uri, response, cnonce, and opaque.
> For historical reasons, a sender MUST NOT generate the quoted string syntax for the following parameters: algorithm, qop, and nc.

## 現状

`src/auth.rs` 917 行目付近の `format_auth_params(params: &[(String, String)]) -> String` は `needs_quoting(value)`（値が空、または token 文字以外を含むか）だけで囲み判定を行っている。この関数は `DigestAuth::to_header_value`（446 行目付近） / `DigestChallenge::to_header_value`（504 行目付近） / `BearerChallenge::to_header_value`（603 行目付近） の 3 箇所で共用されている。

```rust
fn format_auth_params(params: &[(String, String)]) -> String {
    let mut parts = Vec::new();
    for (name, value) in params {
        if needs_quoting(value) {
            parts.push(alloc::format!("{}=\"{}\"", name, escape_quotes(value)));
        } else {
            parts.push(alloc::format!("{}={}", name, value));
        }
    }
    parts.join(", ")
}
```

## 設計方針

1. `format_auth_params` は `BearerChallenge` 用の汎用 formatter として維持する。Digest 専用の `format_digest_params(params, rules)` を新設し、`DigestAuth::to_header_value` / `DigestChallenge::to_header_value` はこちらを使用する。

2. パラメータの quoting 形式を表す属性テーブルを `src/auth.rs` に導入する。例:
   ```rust
   enum QuotingStyle {
       MustQuoted,    // 常に quoted-string
       MustToken,     // 常に token
       TokenOrQuoted, // 値に応じて自動選択（未知パラメータ・将来の拡張用）
   }
   ```

3. RFC 7616 Section 3.4 に従い、Authorization 用の属性テーブルでは `username` / `realm` / `nonce` / `uri` / `response` / `cnonce` / `opaque` を `MustQuoted`、`algorithm` / `qop` / `nc` を `MustToken` とする。`userhash` は RFC 7616 Section 3.9.2 の例に基づき `MustToken` とする（MUST quoted / MUST NOT quoted の対象外だが、例では unquoted token として出力されている）。`username*` は属性テーブルに `("username*", QuotingStyle::MustToken)` として含め、RFC 8187 ext-value として unquoted で出力する。ただし `username*` の値には `'` 等の通常の token 文字以外を含みうるため、`is_valid_token` による token 検証は行わず、parse 時に `decode_username_ext_value` で検証済みの値をそのまま出力する。未知パラメータは `TokenOrQuoted`（`needs_quoting` フォールバック）とする。

4. RFC 7616 Section 3.3 に従い、WWW-Authenticate 用の属性テーブルでは `realm` / `domain` / `nonce` / `opaque` / `qop` を `MustQuoted`、`stale` / `algorithm` / `charset` / `userhash` を `MustToken` とする。`charset` / `userhash` は RFC 7616 Section 3.9.2 の例に基づく。未知パラメータは `TokenOrQuoted`（`needs_quoting` フォールバック）とする。

5. `username*` は属性テーブルに含めた上で値をそのまま unquoted で出力する。`*` 末尾の汎用判定は避け、`username*` のみを特殊扱いとする。値は `decode_username_ext_value` で検証済みの RFC 8187 ext-value 形式をそのまま出力し、通常の `MustToken` と同様の token 検証は適用しない。

6. `nc` は unquoted 8 桁の 16 進数として出力する（RFC 7616 Section 3.5: "the nc value MUST be exactly 8 hexadecimal digits"）。`format_digest_params` は `nc` の 0 埋めや桁数整形を行わず、保持されている値をそのまま `name=value` で出力する。値の検証・整形は `DigestAuth` を構築する builder 側で行う前提とし、本 issue では出力形式のみを修正する。

7. `qop` は WWW-Authenticate では quoted-string of one or more tokens（RFC 7616 Section 3.3）、Authorization では single token（RFC 7616 Section 3.4）として出力する。いずれもフォーマッタは値をそのまま引用符で囲むか囲まないかのみを行い、カンマ区切り間の空白整形等は行わない。

8. parse 側は既存の寛容な受理を維持する。本 issue は送信側の canonical 形式を修正する。

## 完了条件

- `DigestAuth::to_header_value` / `DigestChallenge::to_header_value` が設計方針に従った正しい quoting 形式で出力すること。
- `format_auth_params` を変更せず、`BearerChallenge::to_header_value` の既存の quoting 判定ロジックが維持されること。
- 既存のパーステスト（`tests/test_auth.rs` / `pbt/tests/prop_auth.rs`）が破綻しないこと。
- 以下の新規テストが追加されること。
  - `DigestAuth::parse("Digest username=alice, realm=r, nonce=n, uri=/, response=resp").to_header_value()` の出力に `username="alice"`、`realm="r"`、`nonce="n"`、`uri="/"`、`response="resp"` が含まれること。
  - `DigestChallenge::parse("Digest realm=r, nonce=n, domain=\"/d1 /d2\", qop=\"auth,auth-int\"").to_header_value()` の出力に `realm="r"`、`nonce="n"`、`domain="/d1 /d2"`、`qop="auth,auth-int"` が含まれること。
  - `DigestAuth::parse(...)` に `algorithm=SHA-256`、`qop=auth`、`nc=00000001`、`userhash=true` を含めた場合、それぞれが unquoted で出力されること。
  - `DigestChallenge::parse(...)` に `algorithm=SHA-256`、`charset=UTF-8`、`userhash=true`、`stale=true` を含めた場合、それぞれが unquoted で出力されること。`qop="auth,auth-int"` は Challenge 用 MustQuoted なので quoted-string として出力されること。
  - `DigestAuth::parse("Digest username*=UTF-8''%E3%83%A6%E3%83%BC%E3%82%B6, realm=r, nonce=n, uri=/, response=resp").to_header_value()` の出力に `username*=UTF-8''%E3%83%A6%E3%83%BC%E3%82%B6` が含まれ、quoted-string 化されていないこと。
  - `realm=""` / `domain=""` など空の `MustQuoted` 値が `name=""` 形式で出力されること。
  - DQUOTE を含む値（例: `username="a\"b"`）とバックスラッシュを含む値（例: `username="a\\b"`）で `"` / `\` が escape された quoted-string としてラウンドトリップすること。
  - PBT: Authorization 用 MustQuoted パラメータ（`username` / `realm` / `nonce` / `uri` / `response` / `cnonce` / `opaque`）を token 文字のみ生成し、出力が必ず `name="value"` 形式になることを検証すること。
  - PBT: Challenge 用 MustQuoted パラメータ（`realm` / `domain` / `nonce` / `opaque` / `qop`）を token 文字のみ生成し、出力が必ず `name="value"` 形式になることを検証すること。
  - PBT: Authorization 用 MustToken パラメータ（`algorithm` / `qop` / `nc` / `userhash`）を有効な token 値で生成し、出力が必ず `name=value` 形式になることを検証すること。例: `algorithm` は `MD5` / `SHA-256` / `SHA-256-sess` 等、`qop` は `auth` / `auth-int`、`nc` は 8 桁 16 進数、`userhash` は `true` / `false`。
  - PBT: Challenge 用 MustToken パラメータ（`stale` / `algorithm` / `charset` / `userhash`）を有効な token 値で生成し、出力が必ず `name=value` 形式になることを検証すること。例: `stale` は `true` / `false`、`charset` は `UTF-8`。

## 解決方法

- 設計方針に従い、`format_digest_params(params, rules)` を新設する。
  - 属性テーブル定数の命名例: `DIGEST_AUTH_QUOTING_RULES` / `DIGEST_CHALLENGE_QUOTING_RULES`。
  - `rules` は定数配列 `&[(&str, QuotingStyle)]` とし、パラメータ名を線形スキャンで lookup する。テーブルキーは `parse_auth_params` がパラメータ名を `to_ascii_lowercase()` して保持することに合わせて小文字とする。not found の場合は `TokenOrQuoted` として扱う。
  - `MustQuoted`: `name="value"` 形式。値に DQUOTE またはバックスラッシュを含む場合は `escape_quotes` で escape する。
  - `MustToken`: `name=value` 形式。値が空または token 文字以外を含む場合は `debug_assert!(is_valid_token(value), "MustToken parameter {} has non-token value: {:?}", name, value)` で検出する。ただし `username*` は RFC 8187 ext-value のため `'` 等の token 文字以外を含みうるが、parse 時に `decode_username_ext_value` で検証済みの値をそのまま出力するため、`is_valid_token` チェックは適用しない。リリースビルドでは `name=value` としてそのまま出力し、値の検証責任は呼び出し側（parse / 将来の builder）に持たせる。
  - `TokenOrQuoted`: 既存の `needs_quoting` ロジックで自動判定する。
- `DigestAuth::to_header_value` では Authorization 用属性テーブル、`DigestChallenge::to_header_value` では Challenge 用属性テーブルを `format_digest_params` に渡す。
- `format_auth_params` は `BearerChallenge` 用にそのまま維持する。ただし、今後 `DigestAuth` / `DigestChallenge` が `format_digest_params` に移行した後も関数名が汎用的なため、コードコメントで「`BearerChallenge` 専用」と明記する（リネームは本 issue のスコープ外）。
- テストは `DigestAuth::parse(...)` / `DigestChallenge::parse(...)` してから `to_header_value()` を呼び出し、再シリアライズ結果を検証する。builder API は本 issue では追加しない。

## 参考: RFC 文面

- RFC 7616 Section 3.9.2（抜粋）
  > algorithm=SHA-512-256, charset=UTF-8, userhash=true
  > username*=UTF-8''J%C3%A4s%C3%B8n%20Doe, nc=00000001, qop=auth
- RFC 8187 Section 3.2.1
  > The value part of an extended parameter (ext-value) is a token...
  > For backwards compatibility with RFC 2231, the encoding defined by this specification deviates from common parameter syntax in that the quoted-string notation is not allowed.
