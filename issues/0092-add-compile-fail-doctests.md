# 構築時検査の compile-fail テストを doctest で整備する

- Priority: Medium
- Created: 2026-05-23
- Model: Opus 4.7

## 目的

issue 0091 で導入する `HeaderName::from_static` / `Method::from_static` /
`Scheme::from_static` (`const fn`) について、**不正リテラルがコンパイルエラーになることを
回帰テストで担保する**仕組みを導入する。

Rust 標準の `/// ```compile_fail` doctest を各 `from_static` メソッドのドキュメントに
直接記述し、`cargo test --doc` で「期待通り fail する」ことを検証する。
外部クレート（trybuild 等）は使用しない。

本テストは `from_static` のコンパイル時検査（`const fn` 内の `panic!`）のみを対象とする。
`new()` のランタイム検証は PBT（0093）の責務。

## 優先度根拠

Medium。0091 と同じ。「コンパイル時検査が壊れた」ことをサイレントに見逃さないための
リグレッション防止層であり、0091 の価値を CI で維持するために必須。0091 完了後すぐに着手する。

## 現状

http11 には現在 `const fn` ベースの構築 API が無いため、本テストは存在しない。

## 設計方針

### 外部依存なし

`compile_fail` doctest は rustdoc の標準機能であり、外部クレート不要。
依存 0 ポリシーを完全に維持できる。

### 先行実装

shiguredo/http2-rs の `ClientStreamId::from_static` / `ServerStreamId::from_static` /
`NonZeroStreamId::from_static` で同じアプローチを採用済み。

### 記述場所

各 `from_static` メソッドの doc comment 内に `compile_fail` ブロックを記述する。
正常系の `const` 構築例も同じ doc comment 内に通常の doctest として記述する。

### HeaderName::from_static の例

```rust
/// const 文脈で生成する
///
/// # 正常系
///
/// ```
/// use shiguredo_http11::HeaderName;
///
/// const HOST: HeaderName = HeaderName::from_static(b"host");
/// const CONTENT_TYPE: HeaderName = HeaderName::from_static(b"Content-Type");
/// ```
///
/// # 不正リテラルの compile-fail 例
///
/// 空のヘッダー名は不正:
///
/// ```compile_fail
/// const _BAD: shiguredo_http11::HeaderName =
///     shiguredo_http11::HeaderName::from_static(b"");
/// ```
///
/// CRLF 注入を含むヘッダー名は不正:
///
/// ```compile_fail
/// const _BAD: shiguredo_http11::HeaderName =
///     shiguredo_http11::HeaderName::from_static(b"host\r\nX-Inject: evil");
/// ```
///
/// NUL バイトを含むヘッダー名は不正:
///
/// ```compile_fail
/// const _BAD: shiguredo_http11::HeaderName =
///     shiguredo_http11::HeaderName::from_static(b"host\0");
/// ```
///
/// コロンを含むヘッダー名は不正:
///
/// ```compile_fail
/// const _BAD: shiguredo_http11::HeaderName =
///     shiguredo_http11::HeaderName::from_static(b"host:name");
/// ```
///
/// 空白を含むヘッダー名は不正:
///
/// ```compile_fail
/// const _BAD: shiguredo_http11::HeaderName =
///     shiguredo_http11::HeaderName::from_static(b"host name");
/// ```
pub const fn from_static(bytes: &'static [u8]) -> Self {
    // ...
}
```

### Method::from_static の例

```rust
/// const 文脈で生成する
///
/// # 正常系
///
/// ```
/// use shiguredo_http11::Method;
///
/// const GET: Method = Method::from_static(b"GET");
/// const POST: Method = Method::from_static(b"POST");
/// ```
///
/// # 不正リテラルの compile-fail 例
///
/// 空のメソッドは不正:
///
/// ```compile_fail
/// const _BAD: shiguredo_http11::Method =
///     shiguredo_http11::Method::from_static(b"");
/// ```
///
/// CR を含むメソッドは不正:
///
/// ```compile_fail
/// const _BAD: shiguredo_http11::Method =
///     shiguredo_http11::Method::from_static(b"GET\r");
/// ```
///
/// LF を含むメソッドは不正:
///
/// ```compile_fail
/// const _BAD: shiguredo_http11::Method =
///     shiguredo_http11::Method::from_static(b"GET\n");
/// ```
pub const fn from_static(bytes: &'static [u8]) -> Self {
    // ...
}
```

### Scheme::from_static の例

```rust
/// const 文脈で生成する
///
/// # 正常系
///
/// ```
/// use shiguredo_http11::Scheme;
///
/// const HTTPS: Scheme = Scheme::from_static(b"https");
/// const HTTP: Scheme = Scheme::from_static(b"http");
/// ```
///
/// # 不正リテラルの compile-fail 例
///
/// 空のスキームは不正:
///
/// ```compile_fail
/// const _BAD: shiguredo_http11::Scheme =
///     shiguredo_http11::Scheme::from_static(b"");
/// ```
///
/// 数字始まりのスキームは不正:
///
/// ```compile_fail
/// const _BAD: shiguredo_http11::Scheme =
///     shiguredo_http11::Scheme::from_static(b"3http");
/// ```
///
/// コロンを含むスキームは不正:
///
/// ```compile_fail
/// const _BAD: shiguredo_http11::Scheme =
///     shiguredo_http11::Scheme::from_static(b"http:");
/// ```
pub const fn from_static(bytes: &'static [u8]) -> Self {
    // ...
}
```

### compile_pass に相当する doctest

正常系（大文字 HeaderName、小文字 Method、大文字 Scheme）は通常の doctest として記述する。
`cargo test --doc` で compile-pass / compile-fail の両方が一括検証される。

## 完了条件

- 各 `const fn from_static` API の doc comment に以下が記述されている:
  - 正常系の通常 doctest（compile-pass 相当）
  - 不正入力の `compile_fail` doctest

  | 型 | compile-fail ケース | compile-pass ケース |
  |---|---|---|
  | `HeaderName` | 空 / CRLF 注入 / NUL / コロン含有 / 空白 | 小文字 / 大文字 |
  | `Method` | 空 / CR / LF | 大文字 / 小文字（任意） |
  | `Scheme` | 空 / 数字開始 / コロン含有 | 小文字 / 大文字 |

- `cargo test --doc` で全 doctest が期待通り pass する
  - `compile_fail` ブロック: コンパイルに失敗すること (= テストとしては成功)
  - 通常ブロック: コンパイルと実行に成功すること (= テストとしては成功)
- 外部クレート（trybuild 等）を追加しないこと
- `CHANGES.md` の `## develop` の `### misc` に `[ADD]` エントリが追加されている

## 解決方法

実装順:

1. 0091 完了後、各 `from_static` メソッドの doc comment に正常系 doctest を追加
2. 各エラーパターンに対応する `compile_fail` doctest を追加
3. `cargo test --doc` で全テスト通過を確認
4. `CHANGES.md` に追記

## 関連

- 0091（HeaderName / Method / Scheme 導入）に依存
- 0091 完了後に着手する
- shiguredo/http2-rs の `stream_id.rs` が先行実装
