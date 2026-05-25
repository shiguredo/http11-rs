# header() / add_header() / set_header() でリテラル文字列を直接受け取れるようにする

- Priority: Medium
- Created: 2026-05-25
- Model: Opus 4.7
- Branch: feature/change-header-accepts-str

## 目的

現在の API は `HeaderName` 型を直接要求するため、リテラルヘッダー名を渡す典型的なケースで冗長な記述を強いている。

```rust
// 現在: 冗長
request.header(HeaderName::from_static(b"Host"), "example.com")?;

// 目標: 旧 API と同等の簡潔さ
request.header("Host", "example.com")?;
```

`HeaderName` 導入の型安全性は維持しつつ、呼び出し側のエルゴノミクスを旧 API 相当に戻す。

## 優先度根拠

API のエルゴノミクスに直結する。現状の記述量は「コードを書くのが面倒になっただけ」であり、リテラルに対するコンパイル時検査の恩恵は `const` 文脈以外では実質ランタイム panic と同等。利用者の体験に直接影響するため Medium。

## 現状

- `Request::header` / `Response::header` / `add_header` / `set_header` は `name: HeaderName` を要求する
- リテラルを渡すには `HeaderName::from_static(b"...")` が必要
- `from_static` は `const fn` なので `const` 文脈ではコンパイル時検査が効くが、メソッドチェイン内での利用は非 const でありランタイム panic と同等
- 結果として旧 API (`&str` 直渡し) より明らかに冗長になっている

## 設計方針

1. `From<&'static str>` を `HeaderName` に実装する
   - 内部で `from_static(s.as_bytes())` を呼ぶ (不正入力は panic)
   - `'static` ライフタイム制約により、動的文字列はコンパイルエラーになる
   - 動的文字列は従来どおり `HeaderName::new()` (Result 返却) へ誘導される

2. `From<&'static [u8]>` も `HeaderName` に実装する
   - `b"..."` リテラルからの変換も可能にする

3. `header` / `add_header` / `set_header` の `name` 引数を `impl Into<HeaderName>` に変更する
   - `HeaderName` 値をそのまま渡すのも引き続き可能
   - `"Host"` / `b"Host"` を直接渡せるようになる

4. `Method` についても同様に `From<&'static str>` / `From<&'static [u8]>` を実装し、`Request::new` / `Request::with_version` の `method` 引数を `impl Into<Method>` に変更する

5. `from_static(b"...")` は `const` 文脈 (定数定義) 専用の位置づけとする

## 完了条件

- `.header("Host", "example.com")` が型検査を通過する
- `.header(HeaderName::from_static(b"Host"), "example.com")` も引き続き動作する
- `Request::new("GET", "/")` が型検査を通過する (Method への暗黙変換)
- 動的な `&str` (非 `'static`) を渡すとコンパイルエラーになる
- 既存テスト・PBT・doctest が全て通過する
- README.md / SKILL.md / lib.rs の doctest コード例を簡潔な形式に更新する

## 解決方法

### HeaderName

```rust
impl From<&'static str> for HeaderName {
    fn from(s: &'static str) -> Self {
        Self::from_static(s.as_bytes())
    }
}

impl From<&'static [u8]> for HeaderName {
    fn from(bytes: &'static [u8]) -> Self {
        Self::from_static(bytes)
    }
}
```

### Method

```rust
impl From<&'static str> for Method {
    fn from(s: &'static str) -> Self {
        Self::from_static(s.as_bytes())
    }
}

impl From<&'static [u8]> for Method {
    fn from(bytes: &'static [u8]) -> Self {
        Self::from_static(bytes)
    }
}
```

### Request / Response の header 系メソッド

```rust
pub fn header(
    mut self,
    name: impl Into<HeaderName>,
    value: impl Into<String>,
) -> Result<Self, EncodeError> {
    self.add_header(name, value)?;
    Ok(self)
}

pub fn add_header(
    &mut self,
    name: impl Into<HeaderName>,
    value: impl Into<String>,
) -> Result<&mut Self, EncodeError> {
    let name = name.into();
    // ... 既存のバリデーション
}
```

### Request::new / with_version

```rust
pub fn new(method: impl Into<Method>, uri: impl Into<String>) -> Result<Self, EncodeError> {
    let method = method.into();
    // ... 既存のバリデーション
}
```

### ドキュメント更新

README.md / SKILL.md / lib.rs のコード例を以下の形式に統一する:

```rust
let request = Request::new("GET", "/")?
    .header("Host", "example.com")?
    .header("Connection", "close")?;
```
