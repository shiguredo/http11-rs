# 0067 fix Set-Cookie の Domain 属性を RFC 1034 subdomain 構文準拠で厳格化する

Created: 2026-05-13
Completed: 2026-05-13
Model: Opus 4.7

## 概要

`SetCookie::parse` の `Domain` 属性処理が「leading dot を 1 つだけ strip し、残りが空でなければ store する」という RFC 6265 Section 5.2.3 の手順をそのまま実装しているため、strip 後の値が `Display` 出力 -> 再 parse で別の値に変化するケースが複数存在し、`fuzz_cookie` のラウンドトリップ assertion `assert_eq!(set_cookie.domain(), reparsed.domain())` が複数の入力でクラッシュする。

具体的には以下 2 系統:

1. **leading dot 複数**: `Domain=..` → 旧実装は strip 後 `"."` を store するが、`Display` 出力 `Domain=.` の再 parse で `None` に縮退する。
2. **non-LDH (空白・NUL・制御文字・非 ASCII) を含む値**: `Domain=. foo` → 旧実装は strip 後 `" foo"` (leading space 付き) を store するが、`Display` 出力 `Domain= foo` の再 parse では `attr_value.trim()` で leading space が削られて `"foo"` に縮退する。

本 issue はどちらも「RFC 6265 Section 4.1.1 の `domain-value = <subdomain>` (RFC 1034 Section 3.5 + RFC 1123 Section 2.1) 構文準拠の validity チェックが parser に欠けている」ことが根本原因と整理し、strip 後の値が

- 空でない
- leading dot が残っていない
- LDH (letter / digit / hyphen) と "." のみで構成されている

すべてを満たさない場合は当該 `Domain` 属性を無視するように parser を厳格化する。これにより `Display` 出力が自分自身で再 parse 可能 (fixed-point) になる。

## 再現手順

crash artifact 2 件を regression seed として `fuzz/corpus/fuzz_cookie/` に保存する (`fuzz/.gitignore` により tracked しないが、local 保持で fuzz 走行時に自動的に拾われる):

- `regression-0067` (846 バイト, leading dot 複数系)
- `regression-0067-non-ldh` (101 バイト, non-LDH 系)

再現は以下:

```
cargo +nightly fuzz run fuzz_cookie fuzz/corpus/fuzz_cookie/regression-0067
cargo +nightly fuzz run fuzz_cookie fuzz/corpus/fuzz_cookie/regression-0067-non-ldh
```

入力 hex は単体テスト `test_set_cookie_domain_multi_leading_dot_roundtrip_closed` / `test_set_cookie_domain_leading_dot_space_roundtrip_closed` の最小再現で永続化する。

### 最小再現入力 1 (leading dot 複数系)

入力:

```
3=; Domain=..
```

`SetCookie::parse(..., 2026)` の結果と Display 再 parse 結果:

| | name | value | domain |
|---|---|---|---|
| Original | `"3"` | `""` | `Some(".")` |
| Display | `"3=; Domain=."` | | |
| Reparsed | `"3"` | `""` | `None` |

### 最小再現入力 2 (non-LDH 系)

入力 (実バイトは `domain=. \x00\x00...`):

```
2n=; domain=. foo
```

| | name | value | domain |
|---|---|---|---|
| Original | `"2n"` | `""` | `Some(" foo")` (leading space) |
| Display | `"2n=; Domain= foo"` | | |
| Reparsed | `"2n"` | `""` | `Some("foo")` (space trimmed) |

両者とも fuzz target の line 68 `assert_eq!(set_cookie.domain(), reparsed.domain())` で失敗する:

```
thread '<unnamed>' panicked at fuzz_targets/fuzz_cookie.rs:68:17:
assertion `left == right` failed
```

## 根本原因

`src/cookie.rs` の `"domain"` 属性ブランチ (該当箇所、修正前):

```rust
"domain" => {
    let d = attr_value.strip_prefix('.').unwrap_or(attr_value);
    if d.is_empty() {
        // RFC 6265 Section 5.2.3: 空の場合は無視すべき (SHOULD)
    } else {
        set_cookie.domain = Some(d.to_ascii_lowercase());
    }
}
```

RFC 6265 Section 5.2.3 / RFC 6265bis Section 5.6.3 の strip 規則は「先頭の %x2E を 1 つだけ削除する」しか規定していないため、上記実装は spec を素直に追従している。しかし、

1. **leading dot 複数**: `Domain=..` → strip → `"."` → 空でないので `Some(".")` で保存。`Display` で `Domain=.` を出力。再 parse: strip → `""` → empty なので無視 → `None`。
2. **non-LDH**: `attr_value` は `parse` 内で `.trim()` 済みだが、これは ASCII whitespace のみを edge で削るので、内部の空白や NUL、または leading dot 直後の空白には何の処理もしない。`Domain=. foo` の attr_value は `". foo"` (`.trim()` は edge のみ)、strip で `" foo"` (leading space)、`Some(" foo")` で保存。`Display` 出力 `Domain= foo` の再 parse では attr_value 抽出時に `.trim()` で leading space が削られ `Some("foo")` に縮退。

どちらも「strip 後の値が `Display` -> 再 parse の往復で変化する」点が共通する。

## RFC 解釈

RFC 6265 Section 4.1.1 の ABNF:

```
domain-av    = "Domain=" domain-value
domain-value = <subdomain>
                ; defined in [RFC1034], Section 3.5, as
                ; enhanced by [RFC1123], Section 2.1
```

RFC 1034 Section 3.5 + RFC 1123 Section 2.1 の `<subdomain>` 構文は「letter / digit / hyphen を含む label を `.` で連結したもの」であり、空白や制御文字、NUL は許容しない。

RFC 6265bis Section 6.3 (IDNA Dependency):

> The Domain attribute MUST be either a host or a domain name with all labels in their punycode form... If any label is not in punycode form (i.e., includes characters outside the LDH set), the cookie SHOULD be rejected.

IDN を含むケースでも全 label は punycode (LDH) で渡されることが要求される。すなわち domain-value が「LDH + dot」のみで構成されることは strict なルールであり、非 LDH を含む domain attribute は SHOULD reject。

RFC 6265 Section 5.2.3 の strip 規則 (`.` を 1 つだけ削除) 自体は変更しない。本 issue は「strip 後の残り値が `<subdomain>` 構文に適合するかを検証する」レイヤーを parser に追加するだけである。

## 修正方針

### 修正方針 1: `src/cookie.rs` の `"domain"` ブランチを厳格化する

`strip_prefix('.')` で leading dot を 1 つ剥がした後、残り値が

- 空 (`is_empty()`)
- leading dot 残り (`starts_with('.')`)
- 非 LDH 文字を含む (`is_valid_domain_value` 失敗)

のいずれかを満たす場合は当該 `Domain` 属性を無視する。

```rust
"domain" => {
    // RFC 6265 Section 4.1.1: domain-value = <subdomain> (RFC 1034 Section 3.5 + RFC 1123 Section 2.1)
    //   = LDH (letter / digit / hyphen) を含む label を "." で連結したもの。
    // RFC 6265 Section 5.2.3 / RFC 6265bis Section 5.6.3:
    //   先頭の "." を 1 つだけ除去し、小文字に変換する。
    // RFC 6265bis Section 6.3 (IDNA Dependency):
    //   Domain attribute は全 label が punycode (LDH) でなければならず、
    //   非 LDH を含む値は reject すべき (SHOULD)。
    let d = attr_value.strip_prefix('.').unwrap_or(attr_value);
    if d.is_empty() || d.starts_with('.') || !is_valid_domain_value(d) {
        // 空 / leading dot 複数 / 非 LDH は無視する
    } else {
        set_cookie.domain = Some(d.to_ascii_lowercase());
    }
}
```

新規ヘルパー:

```rust
/// 有効な domain-value かどうか
fn is_valid_domain_value(s: &str) -> bool {
    s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
}
```

### 修正方針 2: regression seed を local 保存し、最小再現を単体テストに永続化する

- `fuzz/corpus/fuzz_cookie/regression-0067` (846 バイト, leading dot 複数系) を local 配置
- `fuzz/corpus/fuzz_cookie/regression-0067-non-ldh` (101 バイト, non-LDH 系) を local 配置
- 上記 2 件は `fuzz/.gitignore` 配下のため tracked しない。代わりに各々の最小再現を `tests/test_cookie.rs` の roundtrip テストに焼き付け永続化する

### 修正方針 3: 単体テストで意図を明文化する

`tests/test_cookie.rs` に以下のテストを追加する:

- `Domain=..` / `Domain=...` / `Domain=..foo` → `None` (leading dot 複数系)
- `Domain=foo bar` / `Domain=foo\0bar` / `Domain=foo\u{6}bar` / `Domain=. foo` / `Domain=日本.example` → `None` (non-LDH 系)
- `Domain=foo..bar` → `Some("foo..bar")` (中間連続 dot は LDH+dot のみで構成されるため受理、roundtrip 閉じる)
- `Domain=foo.bar.` → `Some("foo.bar.")` (trailing dot 形式、受理)
- `Domain=foo-bar.example` → `Some("foo-bar.example")` (hyphen は LDH 内、受理)
- 各 crash 入力の最小再現で `parse -> to_string -> parse` の roundtrip が閉じることを確認する

## 後方互換性への影響

`SetCookie::parse` が以下の入力で `domain()` の返り値が変わる:

- `Domain=..` : 旧 `Some(".")` → 新 `None`
- `Domain=...` : 旧 `Some("..")` → 新 `None`
- `Domain=..foo` : 旧 `Some(".foo")` → 新 `None`
- 非 LDH (空白・NUL・制御文字・非 ASCII) を含む `Domain=...` : 旧 `Some(...)` → 新 `None`

実運用上、Cookie の Domain attribute に上記のような値を送信する事例は想定されない (RFC 6265 / 6265bis にも明示的に反する) ため影響は限定的だが、API 出力が変わるため `[CHANGE]` 扱いとする。

## 検証手順

1. `cargo +nightly fuzz run fuzz_cookie fuzz/corpus/fuzz_cookie/regression-0067` を実行し、修正前は crash 再現、修正後は完走することを確認する
2. `cargo +nightly fuzz run fuzz_cookie fuzz/corpus/fuzz_cookie/regression-0067-non-ldh` 同上
3. `make fmt && make clippy && make check && make test` をすべて通す
4. `tests/test_cookie.rs` の新規テスト群が pass することを確認する
5. 短時間の fuzz 実走 (`-max_total_time=60`) で他の crash が出ないことを確認する

## 解決方法

- `src/cookie.rs` の `"domain"` 属性ブランチに「strip 後の値が LDH + dot のみで構成され、空でなく、再び `.` で始まらない」ことを検証するロジックを追加した
- 新規ヘルパー `is_valid_domain_value(s: &str) -> bool` を `src/cookie.rs` に追加し、RFC 6265 Section 4.1.1 + RFC 1034 Section 3.5 + RFC 1123 Section 2.1 + RFC 6265bis Section 6.3 の subdomain 構文 (LDH + dot) を判定する
- `tests/test_cookie.rs` に以下のテストを追加した:
  - `test_set_cookie_domain_multi_leading_dot_rejected`
  - `test_set_cookie_domain_non_ldh_rejected`
  - `test_set_cookie_domain_intermediate_dot_preserved`
  - `test_set_cookie_domain_trailing_dot_preserved`
  - `test_set_cookie_domain_hyphen_preserved`
  - `test_set_cookie_domain_multi_leading_dot_roundtrip_closed`
  - `test_set_cookie_domain_leading_dot_space_roundtrip_closed`
- `CHANGES.md` `## develop` に `[CHANGE]` エントリを追加した
- `issues/SEQUENCE` を 0062 に更新した
- 検証: `make fmt && make clippy && make check && make test` 全 pass、`cargo +nightly fuzz run fuzz_cookie -- -max_total_time=60` で 517 万 iter 走破して新規 crash 0
