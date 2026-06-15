# Digest 認証の to_header_value が RFC 7616 の quoting 規則に違反

- Priority: High
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-digest-auth-rfc7616-quoting
- Polished: 2026-06-15

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

2. パラメータの quoting 形式を表す属性テーブルを `src/auth.rs` に **非公開** で導入する (`pub` でも `pub(crate)` でもなく private)。送信側 canonical 化のみで使い、API として外部に公開しない。

   ```rust
   enum QuotingStyle {
       MustQuoted,    // 常に quoted-string。値は escape_quotes で escape する
       MustToken,     // 常に token。値は token 文字のみ (debug_assert! で検証)
       ExtValue,      // 常に unquoted。値の token 検証は行わない (RFC 8187 ext-value 等)
       TokenOrQuoted, // 値に応じて自動選択 (未知パラメータ・将来の拡張用)
   }
   ```

3. RFC 7616 Section 3.4 に従い、Authorization 用の属性テーブルでは `username` / `realm` / `nonce` / `uri` / `response` / `cnonce` / `opaque` を `MustQuoted`、`algorithm` / `qop` / `nc` を `MustToken`、`userhash` を `MustToken` (RFC 7616 Section 3.9.2 の例に基づく)、`username*` を `ExtValue` とする。未知パラメータは `TokenOrQuoted` (`needs_quoting` フォールバック)。

   - `username*` を `ExtValue` で扱うことで、「`MustToken` だが `is_valid_token` 検証は例外的にスキップ」という ad-hoc な例外を排除する。値は `decode_username_ext_value` で parse 時に検証済みの RFC 8187 ext-value 形式をそのまま `name=value` で出力する (`'` や `%xx` を含むため通常の token 検証は不適切)。
   - `*` 末尾の汎用判定はしない。`username*` のみを属性テーブル登録する。

4. RFC 7616 Section 3.3 に従い、WWW-Authenticate 用の属性テーブルでは `realm` / `domain` / `nonce` / `opaque` / `qop` を `MustQuoted`、`stale` / `algorithm` / `charset` / `userhash` を `MustToken` とする。`charset` / `userhash` は RFC 7616 Section 3.9.2 の例に基づく。未知パラメータは `TokenOrQuoted` (`needs_quoting` フォールバック)。

   `domain` は RFC 7616 §3.3 で「quoted, space-separated list of URIs」と定義され空 list の意味は規定されていないが、`DigestChallenge::parse` で空値 (`domain=""`) を受理した場合は、本 issue では parse 結果をそのまま再シリアライズする方針に従い `domain=""` を空 quoted-string として出力する。RFC 違反値の挙動を本 issue で新たに規定する責務はない。

   `MustQuoted` の値に CTL (`%x00-1F` / `%x7F`) が混入した場合、`escape_quotes` (`src/validate.rs`) は CTL を SP に置換する仕様であるため、ラウンドトリップで値が改変される可能性がある。parse 経路 (`parse_auth_params`) は `is_qdtext_char` で CTL を reject するため通常は到達不可能だが、将来 builder API を追加する際にはこの挙動に注意する必要がある (本 issue スコープ外、follow-up issue で扱う)。

5. `qop` の Authorization / WWW-Authenticate での違いを正しく扱う:
   - WWW-Authenticate (Challenge) の `qop` は「one or more comma-separated tokens」を quoted-string で囲む形式 (RFC 7616 §3.3)。例: `qop="auth,auth-int"`。`MustQuoted` で囲むだけでよい (内部値 `auth,auth-int` をそのまま `qop="auth,auth-int"` に出力)。
   - Authorization の `qop` は **single token** (RFC 7616 §3.4 「Note that this is a single token, not a quoted list of alternatives as in WWW-Authenticate.」)。`MustToken` で `name=value` 出力する。`,` を含む値が Authorization 側に流れ込んでくると `MustToken` の `debug_assert!(is_valid_token(...))` で debug ビルドは検知可能。リリースビルドでは `,` を含む値がそのまま出力され構文破綻するため、本 issue では「Authorization 側の `qop` 値は single token 限定」を `DigestAuth::parse` 受理仕様として保証する (RFC 7616 §3.4 への準拠)。これは現状の parse 実装が既に保証している (`parse_auth_params` の token 受理経路)。出力側だけ修正しても parse 側で保証されていれば問題ない。

6. `nc` は unquoted 8 桁の 16 進数として出力する (RFC 7616 Section 3.5: "the nc value MUST be exactly 8 hexadecimal digits")。`format_digest_params` は `nc` の 0 埋めや桁数整形を行わず、保持されている値をそのまま `name=value` で出力する。値の整形・8 桁検証は `DigestAuth` を構築する builder 側で行う前提とし、本 issue では出力形式のみを修正する (PBT も「token 文字のみで構成された値」までを検証し、8 桁 16 進セマンティクスの検証は本 issue 対象外)。

7. parse 側は既存の寛容な受理を維持する。本 issue は送信側の canonical 形式を修正する。`DigestAuth::parse(input).to_header_value()` のラウンドトリップは「parse → serialize → parse 結果のパラメータ集合が初回 parse と一致する」(再 parse 同値性) を保証することを完了条件で検証する。

## 完了条件

- `DigestAuth::to_header_value` / `DigestChallenge::to_header_value` が設計方針に従った正しい quoting 形式で出力すること。
- `format_auth_params` を変更せず、`BearerChallenge::to_header_value` の既存の quoting 判定ロジックが維持されること。
- 既存のパーステスト（`tests/test_auth.rs` / `pbt/tests/prop_auth.rs`）が破綻しないこと。
- 以下の新規テストが追加されること。
  - `DigestAuth::parse("Digest username=alice, realm=r, nonce=n, uri=/, response=resp").to_header_value()` の出力に `username="alice"`、`realm="r"`、`nonce="n"`、`uri="/"`、`response="resp"` が含まれること。
  - `DigestChallenge::parse("Digest realm=r, nonce=n, domain=\"/d1 /d2\", qop=\"auth,auth-int\"").to_header_value()` の出力に `realm="r"`、`nonce="n"`、`domain="/d1 /d2"`、`qop="auth,auth-int"` が含まれること。
  - `DigestAuth::parse(...)` に `algorithm=SHA-256`、`qop=auth`、`nc=00000001`、`userhash=true` を含めた場合、それぞれが unquoted で出力されること。
  - `DigestChallenge::parse(...)` に `algorithm=SHA-256`、`charset=UTF-8`、`userhash=true`、`stale=true` を含めた場合、それぞれが unquoted で出力されること。`qop="auth,auth-int"` は Challenge 用 MustQuoted なので quoted-string として出力されること。
  - `DigestAuth::parse("Digest username*=UTF-8''%E3%83%A6%E3%83%BC%E3%82%B6, realm=r, nonce=n, uri=/, response=resp").to_header_value()` の出力に `username*=UTF-8''%E3%83%A6%E3%83%BC%E3%82%B6` が含まれ、quoted-string 化されていないこと (`ExtValue` 経路の検証)。
  - `realm=""` / `domain=""` など空の `MustQuoted` 値が `name=""` 形式で出力されること。
  - DQUOTE を含む値 (例: `realm="a\"b"`) とバックスラッシュを含む値 (例: `realm="a\\b"`) で `"` / `\` が escape された quoted-string としてラウンドトリップすること。
  - ラウンドトリップ再 parse 同値性: 上記すべての例について `DigestAuth::parse(input).to_header_value()` の結果をもう一度 `DigestAuth::parse(...)` した時、初回 parse 結果と同じパラメータ集合 (名前と値) が得られること。
  - PBT: MustQuoted パラメータを token 文字のみで生成し、出力が必ず `name="value"` 形式になることを検証する (Authorization 用と Challenge 用の 2 ケース)。
  - PBT: MustToken パラメータを token 文字のみで生成し、出力が必ず `name=value` 形式になることを検証する (Authorization 用と Challenge 用の 2 ケース)。各値の意味的妥当性 (`nc` の 8 桁 16 進、`algorithm` の MD5 / SHA-256 など) は本 issue の検証対象外で、「token 文字のみで構成された任意の値」をジェネリックに生成すれば十分。
- `CHANGES.md` の `## develop` セクションに以下の `[FIX]` エントリを `[ADD]` の下、`### misc` の上に追加する。`shiguredo-changelog` 規約に従う。
  - `[FIX] DigestAuth / DigestChallenge の to_header_value が RFC 7616 Section 3.3 / 3.4 の quoting 規則 (MUST quoted / MUST NOT quoted) に違反していた問題を修正する。`

## 解決方法

- 設計方針に従い、`format_digest_params(params, rules)` を新設する。
  - 属性テーブル定数の命名例: `DIGEST_AUTH_QUOTING_RULES` / `DIGEST_CHALLENGE_QUOTING_RULES`。両方とも `src/auth.rs` 内のモジュール private 定数とする。
  - `rules` は定数配列 `&[(&str, QuotingStyle)]` とし、パラメータ名を線形スキャンで lookup する。テーブルキーは `parse_auth_params` (`src/auth.rs:782`) がパラメータ名を `to_ascii_lowercase()` して保持することに合わせて小文字とする。not found の場合は `TokenOrQuoted` として扱う。
  - `MustQuoted`: `name="value"` 形式。値に DQUOTE またはバックスラッシュを含む場合は `escape_quotes` で escape する。
  - `MustToken`: `name=value` 形式。値が空または token 文字以外を含む場合は `debug_assert!(is_valid_token(value), "MustToken parameter {} has non-token value: {:?}", name, value)` で検出する。リリースビルドではそのまま出力するが、parse 側が token 受理経路 (`parse_auth_params`) でしか値を入れないため実際には到達しない。
  - `ExtValue`: `name=value` 形式で unquoted。token 検証は行わない (RFC 8187 ext-value は `'` / `%xx` を含むため)。
  - `TokenOrQuoted`: 既存の `needs_quoting` ロジックで自動判定する。
  - `QuotingStyle` enum 自体は `pub` でも `pub(crate)` でもなく完全に private とする (外部 API として公開する必要がないため)。
- `DigestAuth::to_header_value` (`src/auth.rs:446`) では Authorization 用属性テーブル、`DigestChallenge::to_header_value` (`src/auth.rs:504`) では Challenge 用属性テーブルを `format_digest_params` に渡す。
- `format_auth_params` (`src/auth.rs:917`) は `BearerChallenge::to_header_value` (`src/auth.rs:603`) 用にそのまま維持する。ただし、`DigestAuth` / `DigestChallenge` が `format_digest_params` に移行した後も関数名が汎用的なため、コードコメントで「`BearerChallenge` 専用」と明記する (リネームは本 issue のスコープ外)。
- テストは `DigestAuth::parse(...)` / `DigestChallenge::parse(...)` してから `to_header_value()` を呼び出し、再シリアライズ結果を検証する。再 parse 同値性も検証する (`DigestAuth::parse(parsed.to_header_value())` のパラメータ集合と初回 parse 結果の一致)。builder API は本 issue では追加しない。
- PBT 用に `pbt/src/lib.rs` に以下の戦略を追加する (proptest の `Strategy`)。命名は既存戦略と揃える (`token_only_string()` 等)。
  - `digest_token_value() -> impl Strategy<Value = String>`: `pbt/src/lib.rs` に追加。RFC 9110 §11.2 の `token` を満たす任意の 1 文字以上の値を生成する (`[!#$%&'*+\-.^_`|~0-9A-Za-z]{1,32}` 程度)。本戦略を MustQuoted / MustToken の両 PBT で共用する。意味的妥当性 (`nc` の 8 桁 16 進、`algorithm` の MD5 / SHA-256 等) の検証は本 issue 対象外なので、本戦略は token 文字のみをジェネリックに生成すれば十分。

## 参考: RFC 文面

- RFC 7616 Section 3.3
  > For historical reasons, a sender MUST only generate the quoted string syntax values for the following parameters: realm, domain, nonce, opaque, and qop.
  > For historical reasons, a sender MUST NOT generate the quoted string syntax values for the following parameters: stale and algorithm.
  > domain: A quoted, space-separated list of URIs ... that define the protection space.
  > qop: This directive is optional, but it is made so only for backward compatibility with RFC 2069 [RFC2069]; it SHOULD be used by all implementations compliant with this version of the Digest scheme. ... Its value is a quoted string of one or more tokens indicating the "quality of protection" values supported by the server.
- RFC 7616 Section 3.4
  > For historical reasons, a sender MUST only generate the quoted string syntax for the following parameters: username, realm, nonce, uri, response, cnonce, and opaque.
  > For historical reasons, a sender MUST NOT generate the quoted string syntax for the following parameters: algorithm, qop, and nc.
  > qop: ... Note that this is a single token, not a quoted list of alternatives as in WWW-Authenticate.
- RFC 7616 Section 3.5
  > the nc value MUST be exactly 8 hexadecimal digits
- RFC 7616 Section 3.9.2 (抜粋)
  > algorithm=SHA-512-256, charset=UTF-8, userhash=true
  > username*=UTF-8''J%C3%A4s%C3%B8n%20Doe, nc=00000001, qop=auth
- RFC 9110 Section 11.2
  > auth-param = token BWS "=" BWS ( token / quoted-string )
- RFC 8187 Section 3.2.1
  > The value part of an extended parameter (ext-value) is a token...
  > For backwards compatibility with RFC 2231, the encoding defined by this specification deviates from common parameter syntax in that the quoted-string notation is not allowed.
