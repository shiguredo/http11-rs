# Digest 認証の to_header_value が RFC 7616 の quoting 規則に違反

- Priority: High
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-digest-auth-rfc7616-quoting
- Polished: {YYYY-MM-DD}

## 目的

`DigestAuth::to_header_value` / `DigestChallenge::to_header_value` が RFC 7616 で規定されたパラメータの quoting 規則に従っていない。相互運用性を損なうため、送信側で必ず quoted-string 形式を出力すべきパラメータと、必ず token 形式を出力すべきパラメータを正しく区別する。

## 優先度根拠

High とする。RFC 7616 Section 3.4 / 3.3 では "MUST only generate the quoted string syntax" / "MUST NOT generate the quoted string syntax" と明記されており、現状は MUST 要件に違反している。実際に `response=resp` / `username=alice` といった形式が生成され、他の HTTP 実装で受理されないリスクがある。

## 現状

`src/auth.rs:917-927` の `format_auth_params` は `needs_quoting(value)`（値が空、または token 文字以外を含むか）だけで囲み判定を行っている。

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

このため以下の問題がある。

- `username` / `realm` / `nonce` / `uri` / `response` / `cnonce` / `opaque` の値が token 文字のみの場合、引用符なしで出力される。
- `algorithm` / `qop` / `nc` の値に token 外文字が含まれる場合、誤って quoted-string で出力される。
- `username*`（RFC 8187 ext-value）は token 形式でなければならないが、`'` や `%` により quoted-string 化される。

## 設計方針

1. `format_auth_params` を汎用 formatter から、Authorization 用と WWW-Authenticate 用の quoting ルールを持つ実装に変更する。
2. RFC 7616 Section 3.4 に従い、Authorization では `username` / `realm` / `nonce` / `uri` / `response` / `cnonce` / `opaque` を常に quoted-string で、`algorithm` / `qop` / `nc` を常に token で出力する。
3. RFC 7616 Section 3.3 に従い、WWW-Authenticate では `realm` / `domain` / `nonce` / `opaque` / `qop` を常に quoted-string で、`stale` / `algorithm` を常に token で出力する。
4. `username*` は RFC 8187 ext-value として unquoted token で出力する。
5. parse 側は既存の寛容な受理を維持し、送信側の canonical 形式を修正する。

## 完了条件

- `DigestAuth::to_header_value` / `DigestChallenge::to_header_value` が RFC 7616 の quoting 規則に従った出力を行うこと。
- `response` / `username` / `realm` / `nonce` 等の値が token 文字のみでも `"..."` で囲まれること。
- `algorithm=SHA-256` / `qop=auth` / `nc=00000001` が quoted-string にならないこと。
- `username*=UTF-8''...` が quoted-string にならないこと。
- 既存のパーステストが破綻しないこと。
- quoting 規則を検証する新規テストが追加されること。

## 解決方法

- `src/auth.rs` に Digest Authorization 用と Digest Challenge 用のパラメータ属性テーブルを導入する（例: enum で `MustQuoted` / `MustToken` / `TokenOrQuoted` を表現）。
- `format_auth_params` にその属性テーブルを渡し、正しい形式で出力する。
- `DigestAuth::to_header_value` と `DigestChallenge::to_header_value` それぞれで適切な属性テーブルを指定する。
- `username*` 出力時は `*` 付きパラメータ名を検出し、値をそのまま token として出力する。
- `tests/test_auth.rs` / `pbt/tests/prop_auth.rs` に以下を追加する:
  - `DigestAuth::to_header_value()` の出力に `response="..."` / `username="..."` が含まれること。
  - `algorithm` / `qop` / `nc` が unquoted であること。
  - `username*` が unquoted であること。
