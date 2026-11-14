# SetCookie の Path 検証不足と builder 無検証を修正する

- Priority: High
- Created: 2026-07-08
- Completed: {YYYY-MM-DD}
- Model: Grok 4.5
- Branch: feature/fix-setcookie-path-and-builder-validation
- Polished: 2026-09-08
- Refreshed: 2026-11-14

## 目的

`SetCookie` の Path 文字種検証、`with_domain` / `with_path` の入力検証と `Result` 化、`with_max_age` の負値クランプは `4d73a07` で実装済みである。
残る `with_domain` / `with_path` の前後 OWS 処理を parse と揃え、`Display` 再 parse 時に格納値が変わる問題を修正する。既存の設計方針にある `trim_ows` の適用、未検証の回帰条件、Path 検証の変更履歴への記載を完了させる。

## 優先度根拠

High とする。

- `with_path("/foo ")` は末尾 SP を保持するが、`Display` の出力を再 parse すると `"/foo"` になり、格納値の round-trip が破綻する
- parse は属性値に `trim_ows` を適用する一方、builder は適用しない。前後 SP / HTAB 付きの Path / Domain に対する受理・格納値が一致しない
- 起票時の属性注入につながる Path / Domain の無検証と Max-Age 負値の問題は実装済みの検証・クランプで解消しているが、前後 OWS の非対称は残っている

## 現状

### Path の parse (`src/cookie.rs`)

```rust
"path" if is_valid_path_value(attr_value) => {
    set_cookie.path = Some(attr_value.to_string());
}
```

- `is_valid_path_value` が `/` 始まり・非空と `%x20-3A / %x3C-7E` の文字集合を検証し、CTL / obs-text を含む属性値を無視する。属性値には先に `trim_ows` を適用している
- RFC 6265 Section 4.1.1: `path-value = <any CHAR except CTLs or ";">`。`CHAR` は USASCII (`%x01-7F`) なので、CTL と `;` を除くと `%x20-3A / %x3C-7E` (印字可能 ASCII から `;` を除く) になる
- RFC 6265bis Section 4.1.1: `path-value = *av-octet`、`av-octet = %x20-3A / %x3C-7E`。RFC 6265 と同じ集合になる
- `SetCookie::parse` は入力を先に `;` で分割するため、`attr_value` に `;` は到達しない。`;` の拒否が必要なのは builder の経路 (`Display` は格納済み値の不変条件に依存する)

### builder

```rust
pub fn with_max_age(mut self, max_age: i64) -> Self { ... }
pub fn with_domain(mut self, domain: &str) -> Result<Self, CookieError> { ... }
pub fn with_path(mut self, path: &str) -> Result<Self, CookieError> { ... }
```

`with_path` は `is_valid_path_value` を使って検証し、`with_domain` は先頭 `.` を 1 つ除去したうえで非空・複数先頭 `.`・LDH の検証と小文字化を行う。不正値は `CookieError::InvalidAttribute` になる。`with_max_age` は負値を 0 にクランプする。

ただし `with_path` / `with_domain` は入力に `trim_ows` を適用していない。`Display` は格納値をそのまま出力するため、`with_path("/foo ")` の末尾 SP は再 parse 時に失われる。`with_domain(" .Example.COM ")` は builder でエラーになるが、`SetCookie::parse("name=value; Domain= .Example.COM ", 2026)` は Domain として `"example.com"` を保持する。

`tests/test_cookie.rs` には不正な相対 Path・内部 HTAB、Domain の先頭 `.` 除去・小文字化・複数先頭 `.` 拒否、負の Max-Age のテストがある。`tests/test_cookie.rs` / `pbt/tests/prop_cookie.rs` の builder 呼び出しも `Result` 化に追従済みである。ただし PBT の `cookie_path` は `/[a-zA-Z0-9_-]*`、`cookie_domain` は小文字の通常形のみを生成し、前後 OWS や Path 末尾 SP による不一致を検証していない。

`CHANGES.md` の `## develop` には builder の署名変更と検証強化の `[CHANGE]` がある。parse Path の文字種検証を記載する `[FIX]` は未追記である。

### 緩和経路 (誤解しないこと)

現在の Path / Domain の検証は、HTTP field-value の検証を経由しない builder / parse の直接呼び出しにも適用される。起票時に挙げた CTL / obs-text / `;` の不正な属性値を builder で格納・再出力できる経路は解消済みである。残る差異は次のとおりである:

1. `with_path` は前後 OWS を除去せず、前方 SP / HTAB や末尾 HTAB を含む値を拒否する一方、末尾 SP は保持する
2. `with_domain` は前後 OWS を含む値を拒否する一方、parse は除去してから検証する
3. `with_path` が保持した末尾 SP は `Display` 再 parse で失われる

## 設計方針

1. **Path (parse、実装済み)**
   - `/` 始まり・非空に加え、path-value の文字種を検証する
   - RFC 6265 の `CHAR` は USASCII であり、RFC 6265 / 6265bis の path-value はともに `%x20-3A / %x3C-7E` (印字可能 ASCII から `;` を除く) に一致する。CTL / obs-text / `;` を含む値は属性無視 (`None` 維持)
   - `;` は `parse` が先に分割するため `attr_value` には到達しない。この経路の `;` は builder で拒否し、`Display` は格納済み値の不変条件に依存する
2. **builder**
   - `with_path` の文字種 (`%x20-3A / %x3C-7E`)・先頭 `/` の検証と `CookieError::InvalidAttribute` の返却は実装済み。残る入力の `trim_ows` を parse と同じく検証前に適用する。それ以外の正規化はしない
   - `with_domain` の先頭 `.` を 1 つ除去・小文字化・非空・`.` 始まりでない・`is_valid_domain_value` の検証とエラー返却は実装済み。残る `trim_ows` を先頭 `.` の除去前に適用する。これにより parse と builder の格納値が一致し `Display` 再 parse の round-trip が閉じる
   - `with_max_age` の負値を 0 にクランプして `Self` を返す処理は実装済み (署名変更なし)
   - `with_path` / `with_domain` の `Result<Self, CookieError>` 化と `CHANGES.md` の `[CHANGE]` は反映済み
3. **Display**
   - 不変条件として「格納済み値は既に valid」を前提にしてよいが、debug_assert または再検証で防御してもよい

Domain のラベル制約強化 (空ラベル禁止等) は本 issue に含めず、必要なら別 issue とする。

## 完了条件

- `with_path` / `with_domain` が入力の前後 SP / HTAB を除去してから検証・正規化し、parse と受理・格納値が一致する
- `Display` の round-trip が前後 OWS 付きの正当値でも破綻しない。`with_path("/foo ")` は `"/foo"`、`with_domain(" .Example.COM ")` は `"example.com"` を保持し、再 parse 後も一致する
- 上記 OWS 処理と round-trip、および元の完了条件で求める文字種・不正値のうち未検証の条件をテストで固定する。parse Path の CTL / obs-text 無視、builder Path の CR / LF / `;` / obs-text 拒否、builder Domain の空値・非 LDH 拒否が残っている。実装済みの検証・クランプと既存テストは維持する
- `cargo test --workspace` が通る
- `CHANGES.md` に `[FIX]` (parse Path の文字種検証) を追記する。builder 署名の `[CHANGE]` は記載済みである

## 解決方法

1. `src/cookie.rs` の `SetCookie::with_path` / `SetCookie::with_domain` で、既存の検証・正規化前に parse と同じ `trim_ows` を適用する。path-value 検証ヘルパと parse への適用、builder の `Result` 化・検証、Max-Age の負値クランプは実装済みの処理を維持する
2. `tests/test_cookie.rs` / `pbt/tests/prop_cookie.rs` に前後 OWS と round-trip、未検証の文字種の回帰条件を反映する。builder 呼び出しの `Result` 化への追従は完了済みである (examples に builder 利用はない)
3. `CHANGES.md` に未記載の Path 文字種検証の `[FIX]` を追記する

## 影響範囲

- `src/cookie.rs`
- `tests/test_cookie.rs` / 関連 PBT
- `SetCookie` builder を使う外部利用者 (署名変更は反映済み。残作業では前後 OWS の受理・格納値が変わる)
