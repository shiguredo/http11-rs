# SetCookie の Path 検証不足と builder 無検証を修正する

- Priority: High
- Created: 2026-07-08
- Completed: {YYYY-MM-DD}
- Model: Grok 4.5
- Branch: feature/fix-setcookie-path-and-builder-validation
- Polished: 2026-09-08

## 目的

`SetCookie` の `Path` 属性が RFC 6265 / 6265bis の path-value 制約を満たさず、かつ `with_domain` / `with_path` / `with_max_age` が入力検証なしで危険な値を保持・再出力できる問題を修正する。

## 優先度根拠

High とする。

- `Display` が属性値をそのまま連結するため、builder 経由で CR / LF / `;` 等を入れると Set-Cookie 行が壊れる。特に `;` は encoder の field-value 検証を通過して属性を分割させる (attribute injection) ため、生成経路の安全度が低い
- parse 側は Domain を LDH 検証する一方、Path は非空・`/` 始まりのみで文字種を検証しておらず、builder は一切検証しない。受信と生成の安全度が非対称
- 過去 issue で Max-Age 負値クランプと Path 空値は直したが、CTL / obs-text / `;` の文字種検証と builder 検証は未対応

## 現状

### Path の parse (`src/cookie.rs`)

```rust
"path" if !attr_value.is_empty() && attr_value.starts_with('/') => {
    // RFC 6265 Section 5.2.4: 空の attribute-value や
    // / 始まりでない値は default-path を使うべき (None を維持する)
    set_cookie.path = Some(attr_value.to_string());
}
```

- `/` 始まりかつ非空であることしか見ていない
- RFC 6265 Section 4.1.1: `path-value = <any CHAR except CTLs or ";">`。`CHAR` は USASCII (`%x01-7F`) なので、CTL と `;` を除くと `%x20-3A / %x3C-7E` (印字可能 ASCII から `;` を除く) になる
- RFC 6265bis Section 4.1.1: `path-value = *av-octet`、`av-octet = %x20-3A / %x3C-7E`。RFC 6265 と同じ集合になる
- `SetCookie::parse` は入力を先に `;` で分割するため、`attr_value` に `;` は到達しない。`;` の拒否が必要なのは builder の経路 (`Display` は格納済み値の不変条件に依存する)

### builder

```rust
pub fn with_max_age(mut self, max_age: i64) -> Self { ... }
pub fn with_domain(mut self, domain: &str) -> Self { ... }
pub fn with_path(mut self, path: &str) -> Self { ... }
```

いずれも検証なし。`Display` は `Domain` / `Path` / `Max-Age` を素通しで出力する。

### 緩和経路 (誤解しないこと)

HTTP メッセージとして decoder が受けた `Set-Cookie` ヘッダ値は、先に `is_valid_field_value` で CR / LF / 多くの CTL を拒否する。
したがって **wire 上の header field 経由だけ** では Path に生の `\r\n` は入りにくい。
生成側でも `Response::add_header` / `set_header` は `is_valid_field_value` で CR / LF を拒否するため、encoder を経由する CR/LF インジェクションは防がれる。
ただし encoder は `;` を field-value として許可するため属性分割 (attribute injection) は防げず、`Display` 出力を encoder を介さず直接ソケットへ書く経路では CR/LF も素通しになる。
それでも次は残る:

1. builder による生成経路 (本命)
2. `SetCookie::parse` を HTTP field-value 検証なしで呼ぶ経路
3. RFC path-value 非準拠 (正しさ)

## 設計方針

1. **Path (parse)**
   - `/` 始まり・非空に加え、path-value の文字種を検証する
   - RFC 6265 の `CHAR` は USASCII であり、RFC 6265 / 6265bis の path-value はともに `%x20-3A / %x3C-7E` (印字可能 ASCII から `;` を除く) に一致する。CTL / obs-text / `;` を含む値は属性無視 (`None` 維持)
   - `;` は `parse` が先に分割するため `attr_value` には到達しない。この経路の `;` は builder で拒否し、`Display` は格納済み値の不変条件に依存する
2. **builder**
   - `with_path` は parse と同じく入力を `trim_ows` したうえで path-value の文字種 (`%x20-3A / %x3C-7E`) と先頭 `/` を検証し、失敗時は `CookieError::InvalidAttribute` を返す。それ以外の正規化はしない
   - `with_domain` は parse と同じ正規化 (`trim_ows` → 先頭 `.` を 1 つ除去 → 小文字化) を行ったうえで、parse と同じ条件 (正規化後が非空・`.` 始まりでない・`is_valid_domain_value` を満たす) を検証し、失敗時は `CookieError::InvalidAttribute` を返す。これにより parse と builder の格納値が一致し `Display` 再 parse の round-trip が閉じる
   - `with_max_age` は parse と同じく負値を 0 にクランプし、`Self` を返す (署名変更なし)
   - `with_path` / `with_domain` は `Self` 返却 → `Result<Self, CookieError>` の破壊的変更になるため `CHANGES.md` に `[CHANGE]` を書く
3. **Display**
   - 不変条件として「格納済み値は既に valid」を前提にしてよいが、debug_assert または再検証で防御してもよい

Domain のラベル制約強化 (空ラベル禁止等) は本 issue に含めず、必要なら別 issue とする。

## 完了条件

- `SetCookie::parse` が Path の attribute-value に path-value 外の文字 (CTL / obs-text) を含む場合、その属性を無視する (`None` 維持)
- `with_path` が path-value 外の文字 (CTL / obs-text / `;`) や先頭 `/` でない値でエラーになる
- `with_domain` が不正値 (正規化後に空・`.` 始まり・非 LDH) でエラーになり、先頭 `.` や大文字は parse と同じ正規化 (先頭 `.` 1 つ除去 + 小文字化) を経て格納される
- `with_max_age` は負値を 0 にクランプし parse と一致する
- 正当な Path (`/`, `/foo`) / Domain / Max-Age は従来どおり設定できる
- `Display` の round-trip が正当値で破綻しない (`.Example.COM` のような非正規形も builder 通過後の正規化値で round-trip する)
- 単体テストで上記を固定し、既存の PBT / 単体テストの builder 呼び出しを `Result` 化に追従させる
- `cargo test --workspace` が通る
- `CHANGES.md` に `[CHANGE]` (builder 署名) および `[FIX]` (parse Path の文字種検証) を追記する

## 解決方法

1. path-value 検証ヘルパ (`%x20-3A / %x3C-7E`) を `cookie.rs` に追加する
2. parse の Path 分岐で検証を呼ぶ
3. `with_path` / `with_domain` を `Result` 化し、両者に parse と同じ `trim_ows` を適用する。`with_domain` はさらに parse と同じ正規化と検証 (正規化後が非空・`.` 始まりでない・LDH) を行う。エラーは `CookieError::InvalidAttribute` を返す。`with_max_age` は負値を 0 にクランプして `Self` のままとする
4. 呼び出し側 (`tests/test_cookie.rs` / `pbt/tests/prop_cookie.rs`) を更新する (examples に builder 利用はない)
5. `tests/test_cookie.rs` に回帰テストを追加する
6. `CHANGES.md` を更新する

## 影響範囲

- `src/cookie.rs`
- `tests/test_cookie.rs` / 関連 PBT
- `SetCookie` builder を使う外部利用者 (署名変更)
