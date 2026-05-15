# escape_quotes の CTL 検証を debug_assert! から常時有効なスペース置換に変更する

- Priority: High
- Created: 2026-05-15
- Model: deepseek v4-pro

## 目的

`escape_quotes()` (`src/validate.rs:385-398`) が CTL 文字 (CR/LF/NUL/0x01-08/0x0B-0C/0x0E-1F/0x7F) の検出に `debug_assert!` を使用しており、release ビルドでは完全にバイパスされる。全呼び出し元 (auth / accept / content_type / expect / content_disposition の Display 実装またはその呼び出し連鎖内、計 9 箇所) は入力値検証を `escape_quotes` に依存しているため、release ビルドでは CTL 文字がそのまま出力され HTTP Response Splitting (CWE-113) の攻撃経路になる。

## 優先度根拠

- 全 builder API に波及する HTTP Response Splitting 経路
- `debug_assert!` は CI の release ビルドでは検出されない
- RFC 9110 Section 5.5（行 1606-1611）は CR / LF / NUL に対して "MUST either reject the message or replace each of those characters with SP" と規定しており、SP 置換は RFC に準拠した対応である
- escape_quotes の `debug_assert!` 方式は issue 0063 で導入されたが、0063 は parse 側（受信側）の reject を主眼としており、escape 側の防御層は暫定措置だった。本 issue はその暫定措置を常時有効な RFC 準拠の実装に昇格させる

## 現状

`src/validate.rs:385-398`:
```rust
pub(crate) fn escape_quotes(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        debug_assert!(
            is_quoted_pair_char(c),
            "CTL char (CR/LF/NUL/0x01-08/0x0B-0C/0x0E-1F/0x7F) must be rejected before reaching escape_quotes"
        );
        if c == '"' || c == '\\' {
            result.push('\\');
        }
        result.push(c);
    }
    result
}
```

`debug_assert!` は `#[cfg(debug_assertions)]` でのみ有効であり、release ビルドでは CTL 文字が `"` や `\` のエスケープ処理だけを経てそのまま出力される。

呼び出し元（全 9 箇所）:
- `src/auth.rs:332` `WwwAuthenticate::fmt` - Basic realm
- `src/auth.rs:334` `WwwAuthenticate::fmt` - charset
- `src/auth.rs:918` `format_auth_params` - Digest/Bearer 全パラメータ（`DigestAuth` / `DigestChallenge` / `BearerChallenge` の `to_header_value()` から呼ばれる）
- `src/content_disposition.rs:315` `Display` - name
- `src/content_disposition.rs:319` `Display` - filename
- `src/content_disposition.rs:327` `Display` - カスタムパラメータ
- `src/accept.rs:232` `Display` - MediaRange parameter
- `src/content_type.rs:211` `Display` - ContentType parameter
- `src/expect.rs:173` `Display` - Expectation value

## 設計方針

### 1. CTL 文字を `' '` (SP) に置換する

RFC 9110 Section 5.5（行 1606-1611）:

> Field values containing CR, LF, or NUL characters are invalid and
> dangerous, due to the varying ways that implementations might parse
> and interpret those characters; a recipient of CR, LF, or NUL within
> a field value MUST either reject the message or replace each of those
> characters with SP before further processing or forwarding of that
> message.

CR / LF / NUL に対して MUST で "reject or replace with SP" が規定されている。SP 置換は RFC が明示的に認める対応であり、RFC 準拠である。

その他の CTL (0x01-08, 0x0B-0C, 0x0E-1F, 0x7F) については、RFC 9110 Section 5.5（行 1611-1615）:

> Field values containing other CTL characters are also invalid;
> however, recipients MAY retain such characters for the sake of
> robustness when they appear within a safe context (e.g., an
> application-specific quoted string that will not be processed by any
> downstream HTTP parser).

`escape_quotes` の出力先（`WWW-Authenticate`、`Accept`、`Content-Type`、`Expect` 等）はすべて HTTP 標準ヘッダであり、中間プロキシやブラウザが解析するため "safe context" に該当しない。したがってこれらの CTL を retain することは MAY の条件を満たさず、SP 置換が正しい選択である。

### 2. trade-off の認識

SP 置換には以下のリスクがあることを認識した上で採用する:

- `format_auth_params` 経由で Digest 認証の realm 等に CTL が混入した場合、SP 置換後の値がクライアント側の Digest 計算（`response` 値）と不一致になりサイレントに認証失敗する可能性がある
- このリスクは CTL を素通りさせて Response Splitting を許容するリスクと比較して、RFC が明示的に認める防御策であり許容範囲と判断する
- 根本的な解決は builder API への入力値検証であり、別 issue で対応する

### 3. 実装

`debug_assert!` ブロック（現行の L388-391）を削除し、以下のロジックに置き換える:

```rust
for c in s.chars() {
    if !is_quoted_pair_char(c) {
        result.push(' '); // CTL を SP に置換 (RFC 9110 Section 5.5)
        continue;
    }
    if c == '"' || c == '\\' {
        result.push('\\');
    }
    result.push(c);
}
```

### 4. doc コメントの更新

`src/validate.rs:365-384` の doc コメントを、`debug_assert!` の説明から SP 置換の説明に書き換える。RFC 9110 Section 5.5 の "replace each of those characters with SP" と "safe context" 限定句への参照を含める。

### 5. テストの修正

`src/validate.rs:448-458` の `escape_quotes_debug_assert_on_disallowed_ctl` テスト（`#[cfg(debug_assertions)]` 依存）を削除し、以下のテストに置き換える:

- `escape_quotes_replaces_ctl_with_space`: CR (`\r`)、LF (`\n`)、NUL (`\0`)、他の CTL (`\x01`, `\x1F`)、DEL (`\x7F`) を含む文字列を `escape_quotes` に渡し、CTL が `' '` に置換されることを検証する。置換とエスケープの相互作用も確認する（例: `"\x00\""` → `" \""`、`"\x00\\"` → `" \\"`）

## 完了条件

- `escape_quotes("\x00\x0d\x0a")` が release ビルドでも panic せず `"   "` (3 スペース) を返すこと
- `escape_quotes` が任意の CTL（`\x01`-`\x08`, `\x0B`-`\x0C`, `\x0E`-`\x1F`, `\x7F`）を SP に置換すること
- `escape_quotes("hello")` が従来通り `"hello"` を返すこと（正常系の後方互換）
- `escape_quotes("a\"b")` が従来通り `"a\\\"b"` を返すこと（エスケープ処理の後方互換）
- obs-text (`\u{0080}`..=`\u{10FFFF}`) が従来通り通過すること（後方互換）
- `debug_assert!` ブロック（L388-391）が削除されていること
- `#[cfg(debug_assertions)]` 依存の `escape_quotes_debug_assert_on_disallowed_ctl` テストが削除され、常時実行の置換テストに置き換わっていること
- doc コメントが SP 置換の説明に更新され、RFC 9110 Section 5.5 への参照（行 1606-1615）が含まれていること
- `cargo test` で全テストが通過すること
- `cargo test --release` で全テストが通過すること（release ビルドでの検証）
- `CHANGES.md` の `## develop` に `[FIX]` エントリが追加されていること
