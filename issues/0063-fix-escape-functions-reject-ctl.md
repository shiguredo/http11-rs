# 0063 fix escape 関数群で CR / LF / NUL を防御する

Created: 2026-05-14
Model: deepseek-v4-pro

## 概要

以下の escape 関数がエスケープ対象を `"` と `\` のみに限定しており、CR / LF / NUL を raw のまま出力する:

- `src/content_disposition.rs:526-534` (`escape_quoted_string`)
- `src/auth.rs:928-930` (`escape_quotes`)
- `src/content_type.rs:335-337` (`escape_quotes`)
- `src/accept.rs:658-660` (`escape_quotes`)
- `src/expect.rs:250-252` (`escape_quotes`)

RFC 9110 Section 5.5 は field value に CR / LF / NUL を含むことを invalid とし、受信側に reject または SP 置換を MUST で義務付けている。また RFC 9112 Section 3.2 は sender が protocol elements 内で bare CR を生成することを MUST NOT で禁止している。これらの escape 関数を経由して CR / LF / NUL を含む文字列を quoted-string や field-value として生成すると、HTTP Response Splitting (CWE-113) の経路となる。

0061 完了後の世界では、受信側（parse 側）で CR / LF / NUL は reject されるため escape 側に CR / LF / NUL が到達する経路は閉ざされる。本 issue はそれでも防御層として escape 側にも検出を追加する。

## 設計判断

escape 関数は以下のコンテキストから呼ばれ、戻り値に制約がある:

| 関数 | 呼び出し元 | 戻り値制約 |
|---|---|---|
| `content_disposition.rs::escape_quoted_string` | `Display::fmt` | `fmt::Result` (エラーペイロードなし) |
| `auth.rs::escape_quotes` | `to_header_value() -> String` | 公開 API、`String` |
| `content_type.rs::escape_quotes` | `Display::fmt` | `fmt::Result` |
| `accept.rs::escape_quotes` | `Display::fmt` | `fmt::Result` |
| `expect.rs::escape_quotes` | `Display::fmt` | `fmt::Result` |

呼び出し元のシグネチャを変更する大規模な破壊的変更は本 issue のスコープを超える。issue 0036 で確立された「parse 側で reject されるので escape 側に到達しない不変条件を保ち `debug_assert!` を入れる」方針を踏襲する。具体的には `debug_assert!` で開発時に検出し、release ビルドでは通過させる（到達不能経路のため）。

`debug_assert!` 方式を選択する理由:
- 0061 完了後は parse 側で CR/LF/NUL を reject 済みのため escape 側には到達しない
- `to_header_value() -> String` の破壊的変更を回避できる
- Display impl のエラーハンドリング問題を回避できる
- 先行 issue 0036 と方針が一貫する

## 再現手順

1. `ContentDisposition::with_filename("file\r\nInjected-Header: evil\r\n").to_header_value()` を呼ぶ
2. `\r\n` がエスケープされずに raw のまま出力される
3. これを下流に送出すると Header Injection が成立する

注: 0061 完了後は parse 側で CR/LF/NUL が reject されるため上記再現手順は無効化される。

## 対象ファイル

- `src/content_disposition.rs:526-534` (`escape_quoted_string`)
- `src/auth.rs:928-930` (`escape_quotes`)
- `src/content_type.rs:335-337` (`escape_quotes`)
- `src/accept.rs:658-660` (`escape_quotes`)
- `src/expect.rs:250-252` (`escape_quotes`)

## 推奨対応

### content_disposition.rs::escape_quoted_string (char 単位走査)

```rust
fn escape_quoted_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        debug_assert!(c != '\r' && c != '\n' && c != '\0',
            "CR/LF/NUL を含む値を escape しようとした。parse 側で reject されているはず");
        if c == '"' || c == '\\' {
            result.push('\\');
        }
        result.push(c);
    }
    result
}
```

### escape_quotes の 4 重複対応

`auth.rs`, `content_type.rs`, `accept.rs`, `expect.rs` の `escape_quotes` は完全に同一実装。`validate.rs` に `pub(crate) fn escape_quotes(s: &str) -> String` を新設して共通化する:

```rust
/// quoted-string 文字列のエスケープ (送信側)
///
/// CR / LF / NUL は debug_assert! で検出する。
/// 到達不能経路（parse 側で既に reject 済み）のため release ビルドでは通過させる。
pub(crate) fn escape_quotes(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        debug_assert!(c != '\r' && c != '\n' && c != '\0',
            "CR/LF/NUL を含む値を escape しようとした。parse 側で reject されているはず");
        if c == '"' || c == '\\' {
            result.push('\\');
        }
        result.push(c);
    }
    result
}
```

4 モジュールの重複定義を削除し `use crate::validate::escape_quotes;` に置換する。

## テスト戦略

### 単体テスト

`tests/test_validate.rs` (新設または既存):
- `test_escape_quotes_simple` — `"`, `\` のエスケープ
- `test_escape_quotes_normal` — 通常文字がそのまま通る

Debug ビルドで CR/LF/NUL を含む入力で `debug_assert!` が発火することの確認は `#[cfg(debug_assertions)]` で分岐する:

```rust
#[test]
fn test_escape_quotes_crlf_nul_debug_assert() {
    let result = std::panic::catch_unwind(|| {
        crate::validate::escape_quotes("file\r\n");
    });
    if cfg!(debug_assertions) {
        assert!(result.is_err(), "debug ビルドでは panic すべき");
    }
}
```

### Fuzzing

- 既存 fuzz ターゲット (`fuzz_content_disposition`, `fuzz_auth` 等) は Display 経由で escape 関数を経由する。変更不要。

## CHANGES.md

`## develop` の `### misc` に以下を追記する:

```
- [UPDATE] `escape_quotes` の重複実装を `validate.rs` に統合し CR/LF/NUL の debug_assert! 検出を追加する
  - accept / content_type / expect / auth の 4 重複を `validate::escape_quotes` に集約する
  - content_disposition の `escape_quoted_string` も同様の debug_assert! を追加する
  - 防御層として機能し、parse 側 (0061) で reject 済みの経路の閉鎖を保護する
  - @voluntas
```

## ブランチ名

`feature/fix-escape-functions-reject-ctl`

## 0061 との関係

| 観点 | 0061 (本 issue の補完) | 0063 (本 issue) |
|---|---|---|
| 方向 | 受信 (parse) | 送信 (encode) |
| 対象 | accept / content_type / expect の parse_quoted_string | 5 モジュールの escape 関数 |
| 目的 | CR/LF/NUL を含む入力を reject | CR/LF/NUL の出力を debug_assert! で検出 |
| 依存 | なし | 0061 完了後は escape 側への到達経路が閉鎖 |

## 受け入れ基準

- [ ] `content_disposition.rs::escape_quoted_string` に `debug_assert!` が追加されている
- [ ] `validate.rs` に `pub(crate) fn escape_quotes` が新設されている
- [ ] `auth.rs` / `content_type.rs` / `accept.rs` / `expect.rs` の重複定義が削除され `use crate::validate::escape_quotes;` に置換されている
- [ ] `make fmt && make clippy && make check && make test` が pass
- [ ] `#[cfg(debug_assertions)]` テストで CR/LF/NUL 入力時 `debug_assert!` 発火を確認
- [ ] 既存の Display / to_header_value 経路のテストが pass (リグレッション防止)
- [ ] `CHANGES.md` にエントリが追記されている
