# SetCookie の Path 検証不足と builder 無検証を修正する

- Priority: High
- Created: 2026-07-08
- Completed: {YYYY-MM-DD}
- Model: Grok 4.5
- Branch: feature/fix-setcookie-path-and-builder-validation
- Polished: {YYYY-MM-DD}

## 目的

`SetCookie` の `Path` 属性が RFC 6265 / 6265bis の path-value 制約を満たさず、かつ `with_domain` / `with_path` / `with_max_age` が入力検証なしで危険な値を保持・再出力できる問題を修正する。

## 優先度根拠

High とする。

- `Display` が属性値をそのまま連結するため、builder 経由で CR / LF / `;` 等を入れると Set-Cookie 行が壊れ、ヘッダインジェクションの生成経路になる
- parse 側は Domain を LDH 検証するのに Path / builder だけ開放されており、受信と生成の安全度が非対称
- 過去 issue で Max-Age 負値クランプと Path 空値は直したが、CTL / builder 検証は未対応

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
- RFC 6265 Section 4.1.1: `path-value = <any CHAR except CTLs or ";">`
- RFC 6265bis: `av-octet` 系の印字可能 ASCII 制約

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
それでも次は残る:

1. builder による生成経路 (本命)
2. `SetCookie::parse` を HTTP field-value 検証なしで呼ぶ経路
3. RFC path-value 非準拠 (正しさ)

## 設計方針

1. **Path (parse)**
   - `/` 始まり・非空に加え、CTL と `;` を含む値は属性無視 (`None` 維持)
   - 6265bis に合わせて印字可能 ASCII のみに絞るかは実装時に refs を確認して決める
2. **builder**
   - `with_path` / `with_domain` は parse と同水準の検証を行い、失敗時は `Result` にする
   - `with_max_age` は負値を 0 にクランプするか、負値を Err にする (parse の負値→0 と揃える)
   - 破壊的変更 (`Self` 返却 → `Result<Self, CookieError>`) になるため `CHANGES.md` に `[CHANGE]` を書く
3. **Display**
   - 不変条件として「格納済み値は既に valid」を前提にしてよいが、debug_assert または再検証で防御してもよい

Domain のラベル制約強化 (空ラベル禁止等) は本 issue に含めず、必要なら別 issue とする。

## 完了条件

- `SetCookie::parse` が CTL や `;` を含む Path を受理しない (属性無視)
- `with_path` / `with_domain` が不正値でエラーになる
- `with_max_age` の負値扱いが parse と矛盾しない
- 正当な Path (`/`, `/foo`) / Domain / Max-Age は従来どおり設定できる
- Display の round-trip が正当値で破綻しない
- 単体テストで上記を固定する
- `cargo test --all` が通る
- `CHANGES.md` に `[CHANGE]` (builder 署名) および必要なら `[FIX]` (parse Path) を追記する

## 解決方法

1. path-value 検証ヘルパを `cookie.rs` (または `validate`) に追加する
2. parse の Path 分岐で検証を呼ぶ
3. `with_path` / `with_domain` / 必要なら `with_max_age` を `Result` 化する
4. 呼び出し側 (examples / tests) を更新する
5. `tests/test_cookie.rs` に回帰テストを追加する
6. `CHANGES.md` を更新する

## 影響範囲

- `src/cookie.rs`
- `tests/test_cookie.rs` / 関連 PBT
- `SetCookie` builder を使う外部利用者 (署名変更)
