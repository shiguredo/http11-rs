# builder でヘッダー名・メソッドを `'static str` リテラルで受け取り Result で返す

- Priority: Medium
- Created: 2026-05-25
- Model: Opus 4.7
- Branch: feature/add-header-accepts-str-literal

## 関連

- 前提: issue 0091 （`HeaderName` / `Method` / `Scheme` 導入）、0092 （`from_static` の compile-fail doctest）、0093 （`new()` と ABNF 整合の PBT）
- 本 issue は 0091 以降の **builder 経路のエルゴノミクス** を改善する。token 検査集合（RFC 9110 Section 5.6.2 の `token = 1*tchar`）は 0091 と同一で変更しない
- `Scheme` は builder API の引数型として未使用のため **スコープ外**
- RTSP 拡張メソッド: RFC 7826 Section 13 の method token 構文のみ検査（`$` 先頭禁止等は 0091 以前から未検査）

## 目的

0091 で `HeaderName` / `Method` 型を導入した結果、builder では `HeaderName::from_static(b"...")` / `Method::GET` 等の記述が冗長になった。公開ドキュメント・examples の表記を **リテラル + `?` に統一** し、内部では型安全性を維持する。

```rust
// 現在（examples / README の典型）
Request::new(Method::GET, "/")?
    .header(HeaderName::from_static(b"Host"), "example.com")?;

// 目標（builder の正規表記）
Request::new("GET", "/")?
    .header("Host", "example.com")?
    .header("Connection", "close")?;
```

- builder 上の不正 token は **`Result`（`EncodeError`）で返す**。panic 経路を増やさない
- 動的入力（非 `'static`）は引き続き `HeaderName::new()` / `Method::new()` 経由
- `const` 定義の compile-time 拒否は引き続き `from_static`（0092 維持）

## 優先度根拠

Medium。利用者が最頻で触る builder の記述量を減らし、README の「構築時バリデーションは `Result`」と実装を一致させる。型導入（0091）の意味は維持しつつ、表記だけ 0091 以前に近づける follow-up。

## 公開 API の表記方針（利用者向け）

builder / examples / README / SKILL / `lib.rs` doc では次に **統一** する。

| 項目 | 正規表記 | 備考 |
|---|---|---|
| HTTP メソッド | `"GET"` 等の `'static str` リテラル | case-sensitive（`"get"` ≠ `"GET"`）。doc で明記 |
| カスタム / RTSP メソッド | `"DESCRIBE"` 等 | 同上 |
| ヘッダー名 | `"Host"` 等 | case-insensitive だが送信表記を保持 |
| 失敗 | すべて `?` | `EncodeError` |

**examples / README から除外する表記**（API としては後方互換で残してよい）:

- `HeaderName::from_static(b"...")` を builder チェイン内で使う例
- `Method::GET` を `Request::new` 第 1 引数に使う例（`Method::GET` 定数自体は公開 API として維持）

## 失敗経路（3 レーン）

検査ロジックは 0091 / 0093 と同一。変わるのは **builder リテラル経路が panic ではなく `Result` になる** 点のみ。

| 入力 | 経路 | 不正 token 時 |
|---|---|---|
| 動的 `&str` / バイト列 | `HeaderName::new()` / `Method::new()` | `Err(HeaderNameError)` / `Err(MethodError)` |
| `'static` リテラル + builder | `TryFrom<&'static str>` → `try_into()?` | `Err(EncodeError::InvalidHeaderName { .. })` / `InvalidMethod { .. }` |
| `'static` リテラル + `const` 定義 | `from_static(b"...")` | **コンパイル時 panic**（0092 compile-fail） |
| 検証済み `HeaderName` / `Method` 値 | `TryFrom<HeaderName>` / `TryFrom<Method>`（`Error = HeaderNameError` / `MethodError`） | 到達不能 |

**採用しない**: 専用トレイト（`IntoHeaderName` / `IntoMethod`）。`TryFrom` + エラー型統一で十分であり、新規トレイトの導入は API 表面積を不必要に増やす。

**builder リテラルの typo**（例: `"H ost"`）は **コンパイル時には検出できない**（Rust の非 const 呼び出しの限界）。実行時に `Err` となる。compile-time 拒否が必要な場合のみ `const` + `from_static` を使う（README に明記）。

**非 `'static` な `&str` の制約**: `impl TryInto<HeaderName>` は `TryFrom<&'static str>` のみ実装するため、非 `'static` な `&str` はコンパイルエラーになる。汎用ヘルパー関数内で動的な `&str` を builder に渡す場合は `HeaderName::new()` → `TryInto<HeaderName>` の identity impl 経由で渡す必要がある。この制約は README の「3 レーン」説明で明記する。

## 現状

- `Request` / `Response` / `RequestHead` / `ResponseHead` の `header` / `add_header` / `set_header`（`ResponseHead` に `set_header` なし）は `name: HeaderName` を要求
- `Request::new` / `with_version` / `RequestHead::new` / `with_version` は `method: Method` を要求
- builder でリテラルを渡すには `HeaderName::from_static(b"...")` / `Method::GET` 等が必要
- `EncodeError::InvalidHeaderName { name }` / `EncodeError::InvalidMethod { method }` は定義済みだが現在未使用（本 issue で初めて生成経路が追加される）
- `RequestHead::add_header` / `ResponseHead::add_header` の `value` パラメータは `&str`（`Request` / `Response` 側の `impl Into<String>` とは異なる）

## 設計方針

### 1. `HeaderNameError` / `MethodError` に `input` フィールドを追加

未リリース（develop 上）のため破壊的変更ではない。builder でのエラー変換時に元の入力を保持する。

```rust
pub enum HeaderNameError {
    /// 空のヘッダー名
    Empty { input: String },
    /// 不正なバイトを含む
    InvalidByte { byte: u8, position: usize, input: String },
}

pub enum MethodError {
    /// 空のメソッド
    Empty { input: String },
    /// 不正なバイトを含む
    InvalidByte { byte: u8, position: usize, input: String },
}
```

`input` フィールドの生成方法:

- `new(impl AsRef<[u8]>)`: 入力バイト列を `String::from_utf8_lossy` で変換する（非 UTF-8 バイト列は U+FFFD に置換される。完全な復元性より可読性を優先する）
- `TryFrom<&'static str>`: `s.to_string()` でそのまま保持（UTF-8 保証あり）
- `TryFrom<&'static [u8]>`: `String::from_utf8_lossy` を使用

`input` の所有権移動用アクセサ:

```rust
impl HeaderNameError {
    /// エラーの原因となった入力文字列への参照を返す
    pub fn input(&self) -> &str { ... }
    /// エラーの原因となった入力文字列を消費して返す
    pub fn into_input(self) -> String { ... }
}
```

builder 内のエラー変換では `into_input()` を使い二重アロケーションを回避する:

```rust
let name = name.try_into().map_err(|e: HeaderNameError| EncodeError::InvalidHeaderName {
    name: e.into_input(),
})?;
```

`HeaderNameError` / `MethodError` に `core::error::Error` を実装する（`TryFrom` の `Error` 型として公開 API に露出するため、エラーチェーン互換性を確保する）。

`Display` 実装は `input` フィールドを含める:
- `Empty { input }` → `"empty header name: \"\""`
- `InvalidByte { byte, position, input }` → `"invalid byte 0x20 at position 4 in header name: \"host name\""`

### 2. `TryFrom` を `HeaderName` / `Method` に実装

`From` は **実装しない**（builder で panic を混入させるため）。

```rust
impl TryFrom<&'static str> for HeaderName {
    type Error = HeaderNameError;

    fn try_from(s: &'static str) -> Result<Self, Self::Error> {
        let bytes = s.as_bytes();
        if bytes.is_empty() {
            return Err(HeaderNameError::Empty {
                input: s.to_string(),
            });
        }
        for (i, &b) in bytes.iter().enumerate() {
            if !is_tchar(b) {
                return Err(HeaderNameError::InvalidByte {
                    byte: b,
                    position: i,
                    input: s.to_string(),
                });
            }
        }
        Ok(Self(Cow::Borrowed(bytes)))
    }
}

impl TryFrom<&'static [u8]> for HeaderName {
    type Error = HeaderNameError;
    // 同上（input は String::from_utf8_lossy を使用）
    // from_static(b"...") との対称性のため提供する
}
```

`Method` も同形式。成功時は `Cow::Borrowed`（`'static` 入力のアロケーションなし）。

**検査ロジック共通化方針**: `TryFrom` 実装は `is_tchar` 関数を再利用する独自ループとする。`new()` に委譲すると `Cow::Owned` が返るため zero-alloc を維持できない。`from_static` は `const fn` のため非 const 関数を呼べない。3 経路とも `is_tchar` 判定を使うが、ループ構造は経路ごとに異なる（const ループ / エラー時 input 生成の有無 / Cow の種類）ため、共通化可能な部分は `is_tchar` 関数のみ。

検証済み値の受け渡し（identity impl）:

```rust
impl TryFrom<HeaderName> for HeaderName {
    type Error = HeaderNameError;
    fn try_from(value: HeaderName) -> Result<Self, Self::Error> {
        Ok(value)
    }
}
```

`Method` も同様。エラー型を `HeaderNameError` / `MethodError` に統一することで、builder 側の `impl TryInto<HeaderName, Error = HeaderNameError>` と整合する。意味的には `Infallible` が正確だが、Err を返すことはないため実害なし。

### 3. builder を `impl TryInto<HeaderName>` / `impl TryInto<Method>` に拡張

対象（`try_into()` は **field-value 検査より前** に実行）:

| 型 | メソッド | `name` パラメータ | `value` パラメータ | 備考 |
|---|---|---|---|---|
| `Request` | `new`, `with_version` | `impl TryInto<Method>` | — | |
| `Request` | `header`, `add_header`, `set_header` | `impl TryInto<HeaderName>` | `impl Into<String>`（既存維持） | |
| `Response` | `header`, `add_header`, `set_header` | `impl TryInto<HeaderName>` | `impl Into<String>`（既存維持） | |
| `RequestHead` | `new`, `with_version` | `impl TryInto<Method>` | — | |
| `RequestHead` | `header`, `add_header` | `impl TryInto<HeaderName>` | `&str`（既存維持） | `set_header` なし |
| `ResponseHead` | `header`, `add_header` | `impl TryInto<HeaderName>` | `&str`（既存維持） | `set_header` なし |

`RequestHead` / `ResponseHead` の `value` パラメータは `&str` のまま維持する（decoder 側の用途が主でありオーナーシップ移動が不要）。

```rust
// Request / Response の例
pub fn add_header(
    &mut self,
    name: impl TryInto<HeaderName, Error = HeaderNameError>,
    value: impl Into<String>,
) -> Result<&mut Self, EncodeError> {
    let name = name.try_into().map_err(|e: HeaderNameError| EncodeError::InvalidHeaderName {
        name: e.into_input(),
    })?;
    // 以降は既存（field-value の is_valid_field_value 等）
}

// RequestHead / ResponseHead の例
pub fn add_header(
    &mut self,
    name: impl TryInto<HeaderName, Error = HeaderNameError>,
    value: &str,
) -> Result<&mut Self, EncodeError> {
    let name = name.try_into().map_err(|e: HeaderNameError| EncodeError::InvalidHeaderName {
        name: e.into_input(),
    })?;
    // 以降は既存
}
```

`Method` 側は `MethodError` → `EncodeError::InvalidMethod` に map（`e.into_input()` を使用）。`Request::new` / `with_version` / `RequestHead::new` / `with_version` も同形式。

**`byte` / `position` の意図的欠落**: `EncodeError::InvalidHeaderName { name }` / `InvalidMethod { method }` には `byte` / `position` フィールドを含めない。理由: builder のエラーはユーザー向けの簡易表示が目的であり、入力文字列があれば原因特定に十分である。詳細なバイト位置情報が必要な場合は `HeaderName::new()` / `Method::new()` を直接呼んで `HeaderNameError` / `MethodError` を取得する経路を使う。`EncodeError` は `#[non_exhaustive]` のため、将来必要になった場合はフィールド追加で対応可能。

### 4. 後方互換

- 既存の `HeaderName` / `Method` / `from_static` / `Method::GET` 渡しは **引き続きコンパイル可能**（identity `TryFrom` impl による）
- `HeaderNameError` / `MethodError` のフィールド変更は未リリースのため互換性問題なし
- `CHANGES.md` は `[UPDATE]`。AGENTS.md「変更履歴は派生元ブランチとの最終的な差分のみを記載すること」に従い、既存の 0091 `[CHANGE]` エントリ（CHANGES.md line 32-34「`impl Into<String>` から `HeaderName/Method` に変更する」）を本 issue の最終形で上書きする。`HttpHead::headers()` 戻り型変更のエントリ（line 22-24）は本 issue の対象外であり触れない。上書き後の文言例:

```
- [CHANGE] Request/Response/RequestHead/ResponseHead のヘッダー名・メソッド引数を impl TryInto<HeaderName> / impl TryInto<Method> に変更する
  - builder で `"Host"` / `"GET"` 等の `'static str` リテラルを直接渡せるようになる
  - 既存の HeaderName / Method / Method::GET 渡しは引き続きコンパイル可能（identity TryFrom impl）
  - 不正リテラルは Err(EncodeError) で返す（panic しない）
  - @voluntas
```

## 変更ファイル

| ファイル | 変更内容 |
|---|---|
| `src/header_name.rs` | `HeaderNameError` に `input` 追加、`core::error::Error` impl、`input()` / `into_input()` アクセサ、`TryFrom` impl × 3、型 doc |
| `src/method.rs` | 同上 |
| `src/request.rs` | `impl TryInto<_>` シグネチャ変更 |
| `src/response.rs` | `impl TryInto<HeaderName>` シグネチャ変更 |
| `src/decoder/head.rs` | `RequestHead` / `ResponseHead` の `impl TryInto<_>` シグネチャ変更 |
| `README.md` | リテラル表記統一 + 3 レーン説明 |
| `skills/shiguredo-http11/SKILL.md` | 構築型テーブル + コード例 |
| `src/lib.rs` | crate レベル doc |
| `examples/` | すべて `"Host"` / `"GET"` 形式へ |
| `CHANGES.md` | 既存 0091 `[CHANGE]` エントリを最終形で上書き + `[UPDATE]` エントリ追加 |

`tests/` / `pbt/` で `HeaderNameError` / `MethodError` をパターンマッチしている箇所はフィールド変更に伴い修正が必要。`src/encoder.rs` のテストは `HeaderName` / `Method` を直接構築・マッチしていないため修正不要の見込み（実装時に確認すること）。

## テスト方針

| 種別 | 方針 |
|---|---|
| PBT | `prop_header_name.rs` / `prop_method.rs` に「`TryFrom<&'static str>` と `new()` の受理集合が一致する」プロパティを追加する。任意 ASCII 文字列に対して両者の Ok/Err が一致し、Ok の場合は `as_bytes()` が一致すること。既存 `input` フィールド追加に伴うパターンマッチ修正も行う |
| 単体 | 不正入力（`""`, `"host name"`, `"GET\r"`）で `Err` になり `input` / `into_input()` が元の文字列を返すこと。`TryFrom<&'static [u8]>` の非 UTF-8 入力で `input()` が lossy 変換された文字列を返すこと |
| builder | `Request::header("host name", "x")` が `Err(EncodeError::InvalidHeaderName { .. })` を返すこと（panic しないこと）。`RequestHead::header("host name", "x")` も同様 |
| compile_fail | 非 `'static` `&str` を builder に渡すとコンパイルエラー（型 doc の doctest）。0092 の `from_static` compile-fail は **変更しない** |
| Fuzzing | 追加不要 |

## 実装順序

1. `src/header_name.rs` / `src/method.rs` の `HeaderNameError` / `MethodError` に `input` フィールド追加 + `core::error::Error` impl + `input()` / `into_input()` アクセサ + 既存コード修正
2. `src/header_name.rs` / `src/method.rs` に `TryFrom` impl 追加
3. `src/request.rs` / `src/response.rs` / `src/decoder/head.rs` のシグネチャ変更 + エラー map
4. 既存テスト・PBT の `HeaderNameError` / `MethodError` パターンマッチ修正
5. PBT に `TryFrom` / `new()` 等価性プロパティ追加
6. builder 不正リテラルの単体テスト（`tests/test_request.rs` 等）
7. compile-fail doctest（非 `'static` 拒否。型 doc に配置）
8. `README.md` / `SKILL.md` / `lib.rs` / `examples/` 更新
9. `CHANGES.md` 更新

## 完了条件

- `Request::new("GET", "/")?` / `.header("Host", "example.com")?` が型検査を通過する
- `RequestHead::new("GET", "/")?` / `.header("Host", "example.com")?` が型検査を通過する
- `.header("host name", "x")?` が **panic せず** `Err(EncodeError::InvalidHeaderName { .. })` を返す
- エラーの `name` フィールドに元の入力文字列 `"host name"` が含まれる
- 非 `'static` `&str` を builder に渡すとコンパイルエラー（compile-fail doctest）
- `HeaderName` / `Method` 値・`Method::GET` の直接渡しも引き続き動作する（identity `TryFrom` による）
- `RequestHead` / `ResponseHead` も `Request` / `Response` と同じ `TryInto` 拡張（`value` パラメータは `&str` のまま維持）
- `README.md` / `SKILL.md` / `lib.rs` / `examples/` に `HeaderName::from_static` / `Method::GET` を builder 例として **載せない**
- README に 3 レーン（builder `Result` / 動的 `new()` / `const` + `from_static`）と非 `'static` 制約を記載
- `HeaderNameError` / `MethodError` が `core::error::Error` を実装している
- PBT で `TryFrom<&'static str>` と `new()` の受理集合一致が検証されている
- 既存テスト・PBT・doctest が全て通過
- `CHANGES.md` の `## develop` エントリが最終形に更新されている
