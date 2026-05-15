# escape_quotes の CTL 検証を debug_assert! から常時有効なスペース置換に変更する

- Priority: High
- Created: 2026-05-15
- Model: deepseek v4-pro
- Branch: feature/fix-escape-quotes-ctl-bypass

## 目的

`escape_quotes()` (`src/validate.rs:388-391`) が CTL 文字 (CR/LF/NUL/0x01-08/0x0B-0C/0x0E-1F/0x7F) の検出に `debug_assert!` を使用しており、release ビルドでは完全にバイパスされる。全呼び出し元 (auth / accept / content_type / expect / content_disposition の Display 実装、計 9 箇所) は入力値検証を `escape_quotes` に依存しているため、CTL 文字がそのまま出力され HTTP Response Splitting (CWE-113) の攻撃経路になる。

## 優先度根拠

- 全 builder API に波及する HTTP Response Splitting 経路
- `debug_assert!` は CI の release ビルドでは検出されない
- RFC 9110 Section 5.5 は CTL を MUST reject

## 現状

`src/validate.rs:385-398`:
```rust
pub(crate) fn escape_quotes(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        debug_assert!(
            is_quoted_pair_char(c),
            "CTL char (...) must be rejected before reaching escape_quotes"
        );
        if c == '"' || c == '\\' {
            result.push('\\');
        }
        result.push(c);
    }
    result
}
```

呼び出し元 (全 9 箇所、いずれも `fmt::Display` 実装内):
| ファイル | 行 | コンテキスト |
|----------|------|---------------|
| `src/auth.rs` | 332 | `WwwAuthenticate::fmt` - Basic realm |
| `src/auth.rs` | 334 | `WwwAuthenticate::fmt` - charset |
| `src/auth.rs` | 918 | `format_auth_params` - Digest 全パラメータ |
| `src/content_disposition.rs` | 315 | `Display` - name |
| `src/content_disposition.rs` | 319 | `Display` - filename |
| `src/content_disposition.rs` | 327 | `Display` - カスタムパラメータ |
| `src/accept.rs` | 232 | `Display` - MediaRange parameter |
| `src/content_type.rs` | 211 | `Display` - ContentType parameter |
| `src/expect.rs` | 173 | `Display` - Expectation value |

## 設計方針

### 1. CTL 文字を `' '` (space) に置換する（Result 非採用の理由）

対称関数 `parse_quoted_string` (`src/validate.rs:331`) は CTL を `QuotedStringError` で reject する。これと対称に `escape_quotes` も `Result` を返す案もあるが:

- 全 9 箇所の呼び出し元が `fmt::Display` 実装内にあり、`write!()` マクロ内で `?` 伝播するには `fmt::Result` との型整合が必要で実装が複雑化する
- `escape_quotes` は「parse 側で既に reject されているはずの値を、万が一通った場合の**防御層**」という位置づけ（doc コメントにも明記）。この層で新たにエラー経路を追加すると防御層の責務を超えて破壊的変更になる

したがって `String` を返すシグネチャを維持し、CTL 文字は `' '` (space) に置換する。Space は quoted-string の qdtext で許容される文字 (`HTAB / SP`) であり、出力の構造を破壊しない。

### 2. 実装

```rust
for c in s.chars() {
    if !is_quoted_pair_char(c) {
        result.push(' '); // CTL をスペースに置換
        continue;
    }
    if c == '"' || c == '\\' {
        result.push('\\');
    }
    result.push(c);
}
```

### 3. 既存テストの修正

`src/validate.rs:448-458` の `escape_quotes_debug_assert_on_disallowed_ctl` テスト (`#[cfg(debug_assertions)]`) を削除し、代わりに以下を追加する:

- `escape_quotes_replaces_ctl_with_space`: CR/LF/NUL を含む文字列を `escape_quotes` に渡し、CTL が `' '` に置換されることを検証する。このテストは `#[cfg(debug_assertions)]` なしで常時実行する

### 4. 長期的な builder 入力検証

本 issue では escape 側の防御層修正を最優先とする。全 builder API (`with_filename`, `with_parameter`, `WwwAuthenticate::basic` 等) への入力値検証追加は別 issue で対応する。

## 完了条件

- `escape_quotes("\x00\x0d\x0a")` が release ビルドでも panic せず `"   "` (3 スペース) を返すこと
- `escape_quotes("hello")` が従来通り `"hello"` を返すこと（正常系の後方互換）
- `escape_quotes("a\"b")` が従来通り `"a\\\"b"` を返すこと（エスケープ処理の後方互換）
- `#[cfg(debug_assertions)]` 依存の `escape_quotes_debug_assert_on_disallowed_ctl` テストが削除され、常時実行の置換テストに置き換わっていること
- `cargo test` で全テストが通過すること
- `cargo test --release` で全テストが通過すること（release ビルドでの検証）
- `CHANGES.md` の `## develop` に `[FIX]` エントリが追加されていること
