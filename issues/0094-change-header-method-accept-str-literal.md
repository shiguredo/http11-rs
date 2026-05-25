# builder でヘッダー名・メソッドを `'static str` リテラルで受け取り Result で返す

- Priority: Medium
- Created: 2026-05-25
- Model: Opus 4.7
- Branch: feature/add-header-accepts-str-literal

## 関連

- 前提: issue 0091（`HeaderName` / `Method` / `Scheme` 導入）、0092（`from_static` の compile-fail doctest）、0093（`new()` と ABNF 整合の PBT）
- 本 issue は 0091 以降の **builder 経路のエル���ノミクス** を改善する。token 検査集合（RFC 9110 Section 5.6.2 の `token = 1*tchar`）は 0091 と同一で変更しない
- `Scheme` は builder API の引数型として未使用のため **スコープ外**

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
| `'static` リテ���ル + `const` 定義 | `from_static(b"...")` | **コンパイル時 panic**（0092 compile-fail） |
| 検証済み `HeaderName` / `Method` 値 | `TryFrom<HeaderName>` / `TryFrom<Method>`（`Error = HeaderNameError` / `MethodError`） | 到達不能 |

**採用しない**: `From<&'static str>` → `from_static()`。builder で panic を混入させるため。

**採用しない**: 専用トレイト（`IntoHeaderName` / `IntoMethod`）。`TryFrom` + エラー型統一で十分であり、新規トレイトの導入は API 表面積を不必要に増やす。

**builder リテラルの typo**（例: `"H ost"`）は **コンパイル時には検出できない**（Rust の非 const 呼び出しの限界）。実行時に `Err` となる。compile-time 拒否が必要な場合のみ `const` + `from_static` を使う（README に明記）。

## RFC 根拠（検査集合は変更しない）

- `HeaderName`: RFC 9110 Section 5.1 `field-name = token`、Section 5.6.2 `token = 1*tchar`。比較は Section 5.1 に従い case-insensitive
- `Method`: RFC 9110 Section 9.1 `method = token`、Section 5.6.2 `token = 1*tchar`���比較は Section 9.1 に従い case-sensitive
- RTSP 拡張メソッド: RFC 2326 Section 10（token 構文のみ検査。`$` 先頭禁止等は 0091 以前から未検査）

## 現状

- `Request` / `Response` / `RequestHead` / `ResponseHead` の `header` / `add_header` / `set_header`（`ResponseHead` に `set_header` なし）は `name: HeaderName` を要求
- `Request::new` / `with_version` / `RequestHead::new` / `with_version` は `method: Method` を要求
- builder でリテラルを渡すには `HeaderName::from_static(b"...")` / `Method::GET` 等が必要
- `from_static` を builder 内で呼ぶと非 const 文脈では **実行時 panic**（0094 以前から存在。本 issue では builder 正規経路から排除する）

## 設計方針

### 1. `HeaderNameError` / `MethodError` に `input` フィールドを追加

未リリース（develop 上）のため破壊的変更ではない。builder でのエラー変換時に元の入力を保持する。

```rust
pub enum HeaderNameError {
    /// 空のヘッ���ー名
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

`new()` は入力を `String` 化してエラーに含める。`TryFrom<&'static str>` も同様。

### 2. `TryFrom` を `HeaderName` / `Method` に実装

`From` は **実装しない**。

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
}
```

`Method` も同形式。検査は `new()` と同集合とし、成功時は `Cow::Borrowed`（`'static` 入力のアロケーションなし）。`from_static` の実装とロジックを共通化してもよいが、**builder 経路から `from_static` を呼ばない**（panic 経路の混同防止）。

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

| 型 | メソッド |
|---|---|
| `Request` | `new`, `with_version`, `header`, `add_header`, `set_header` |
| `Response` | `header`, `add_header`, `set_header` |
| `RequestHead` | `new`, `with_version`, `header`, `add_header` |
| `ResponseHead` | `header`, `add_header` |

```rust
pub fn add_header(
    &mut self,
    name: impl TryInto<HeaderName, Error = HeaderNameError>,
    value: impl Into<String>,
) -> Result<&mut Self, EncodeError> {
    let name = name.try_into().map_err(|e| EncodeError::InvalidHeaderName {
        name: e.input().to_string(),
    })?;
    // 以降は既存（field-value の is_valid_field_value 等）
}
```

`HeaderNameError` に `input` フィールドがあるため、元の文字列を `EncodeError` に引き継げる。

`Method` 側は `MethodError` → `EncodeError::InvalidMethod` に map。`Request::new` / `with_version` に **method の追加検証は入れない**（現状どおり型構築時に委ねる）。

### 4. `from_static` の位置づけ

| 用途 | API |
|---|---|
| builder（正規） | `"Host"` / `"GET"` リテラル + `TryInto` |
| 動的入力 | `new()?` → 変数を builder に渡す |
| `const` 定数（compile-time 拒否） | `from_static(b"...")` のみ |

builder 例・doctest では `from_static` を使わない。型 doc に 3 レーンを記載する。

### 5. 後方互換

- 既存の `HeaderName` / `Method` / `from_static` / `Method::GET` 渡しは **引き続きコンパイル可能**
- `HeaderNameError` / `MethodError` のフィールド変更は未リリースのため互換性問題なし
- `CHANGES.md` は `[UPDATE]`

## 変更ファイル

| ファイ��� | 変更内容 |
|---|---|
| `src/header_name.rs` | `HeaderNameError` に `input` 追加、`TryFrom` impl × 3、`input()` アクセサ、型 doc |
| `src/method.rs` | 同��� |
| `src/request.rs` | `impl TryInto<_>` シグネチャ変更 |
| `src/response.rs` | `impl TryInto<HeaderName>` シグネチャ���更 |
| `src/decoder/head.rs` | `RequestHead` / `ResponseHead` の `impl TryInto<_>` シグネチャ変更 |
| `README.md` | リテラル表記統一 + 3 レーン説明 |
| `skills/shiguredo-http11/SKILL.md` | 構築型テーブル + コード例 |
| `src/lib.rs` | crate レベル doc |
| `examples/` | すべて `"Host"` / `"GET"` 形式へ |
| `CHANGES.md` | `[UPDATE]` エントリ |

`tests/` / `pbt/` / `src/encoder.rs` の `#[cfg(test)]` は `HeaderNameError` / `MethodError` のフィールド変更に伴い修正が必要。

## テスト方針

| 種別 | 方針 |
|---|---|
| 単体 | `TryFrom<&'static str>` / `TryFrom<&'static [u8]>` が `new()` と等価であること。不正入力（`""`, `"host name"`, `"GET\r"`）で `Err` になり `input` フィールドに元の文字列が入ること |
| builder | `Request::header("host name", "x")` が `Err(EncodeError::InvalidHeaderName { .. })` を返すこと（panic しないこと） |
| compile_fail | 非 `'static` `&str` を builder に渡すとコンパイルエラー（`lib.rs` または型 doc）。0092 の `from_static` compile-fail は **変更しない** |
| PBT | 追加不要（既存 `prop_header_name` / `prop_method` 維持。`input` フィールド追加に伴う修正のみ） |
| Fuzzing | 追加不要 |

## 実装順序

1. `src/header_name.rs` / `src/method.rs` の `HeaderNameError` / `MethodError` に `input` フィールド追加 + 既存コード修正
2. `src/header_name.rs` / `src/method.rs` に `TryFrom` impl 追加
3. `src/request.rs` / `src/response.rs` / `src/decoder/head.rs` のシグネチャ変更 + エラー map
4. 既存テスト・PBT の `HeaderNameError` / `MethodError` パターンマッチ修正
5. builder 不正リテラルの単体テスト（`tests/test_request.rs` 等）
6. compile-fail doctest（非 `'static` 拒否）
7. `README.md` / `SKILL.md` / `lib.rs` / `examples/` 更新
8. `CHANGES.md` 追記

## 完了条件

- `Request::new("GET", "/")?` / `.header("Host", "example.com")?` / `RequestHead::new("GET", "/")?` が型検査を通過する
- `.header("host name", "x")?` が **panic せず** `Err(EncodeError::InvalidHeaderName { .. })` を返す
- エラーの `name` フィールドに元の入力文字列 `"host name"` が含まれる
- 非 `'static` `&str` を builder に渡すとコンパイルエラー（compile-fail doctest）
- `HeaderName` / `Method` 値・`Method::GET` の直接渡しも引き続き動作する
- `RequestHead` / `ResponseHead` も `Request` / `Response` と同じ `TryInto` 拡張
- `README.md` / `SKILL.md` / `lib.rs` / `examples/` に `HeaderName::from_static` / `Method::GET` を builder 例として **載せない**
- README に 3 レーン（builder `Result` / 動的 `new()` / `const` + `from_static`）を記載
- 既存テスト・PBT・doctest が全て通過
- `CHANGES.md` の `## develop` に `[UPDATE]` エントリ追加

## レビュー観点（他 LLM 向けメモ）

- `From<&'static str>` が混入していないか（panic 経路）
- builder 不正リテラルが `Result` であることの単体テスト有無
- `TryFrom` と `new()` / `from_static` の検査集合一致
- examples / README が `"Host"` 形式に統一されているか
- `from_static` を builder 例から排除しつつ、0092 compile-fail が維持されているか
- `RequestHead` / `ResponseHead` の漏れ
- `HeaderNameError` / `MethodError` の `input` フィールドが全バリアントで正しく設定されているか
- identity `TryFrom<HeaderName>` / `TryFrom<Method>` が `Error = HeaderNameError` / `MethodError` を使っているか（`Infallible` ではない）
