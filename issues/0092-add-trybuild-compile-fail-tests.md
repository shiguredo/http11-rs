# 構築時検査の compile-fail テストを trybuild で整備する

- Priority: Medium
- Created: 2026-05-23
- Model: Opus 4.7

## 目的

issue 0091 で導入する `HeaderName::from_static` / `Method::from_static` /
`Scheme::from_static` (`const fn`) について、**不正リテラルがコンパイルエラーになることを
回帰テストで担保する**仕組みを導入する。

`trybuild` クレートを `[dev-dependencies]` として追加し、`tests/trybuild/` 配下に
意図的にコンパイル失敗するソースを配置し、`cargo test` で「期待通り fail する」ことを
検証する。

本テストは `from_static` のコンパイル時検査（`const fn` 内の `panic!`）のみを対象とする。
`new()` のランタイム検証は PBT（0093）の責務。

## 優先度根拠

Medium。0091 と同じ。「コンパイル時検査が壊れた」ことをサイレントに見逃さないための
リグレッション防止層であり、0091 の価値を CI で維持するために必須。0091 完了後すぐに着手する。

## 現状

http11 には現在 `const fn` ベースの構築 API が無いため、本テストは存在しない。

## 設計方針

### 依存追加

```toml
# Cargo.toml（[dev-dependencies] セクションを新設）
[dev-dependencies]
trybuild = "1.0"
```

`trybuild` は `dev-dependencies` 限定でライブラリ本体ビルドには影響しない（依存 0 ポリシーを維持できる）。

### テスト配置

compile-fail と compile-pass をディレクトリで分離する:

```
tests/
  trybuild.rs                                    # ランナー
  trybuild/
    compile_fail/
      header_name_empty.rs
      header_name_empty.stderr
      header_name_crlf.rs
      header_name_crlf.stderr
      header_name_nul.rs
      header_name_nul.stderr
      header_name_with_colon.rs
      header_name_with_colon.stderr
      header_name_with_space.rs
      header_name_with_space.stderr
      method_empty.rs
      method_empty.stderr
      method_cr.rs
      method_cr.stderr
      method_lf.rs
      method_lf.stderr
      scheme_empty.rs
      scheme_empty.stderr
      scheme_digit_start.rs
      scheme_digit_start.stderr
      scheme_with_colon.rs
      scheme_with_colon.stderr
    compile_pass/
      header_name_uppercase.rs  # 大文字の HeaderName は受理されるべき
      method_lowercase.rs       # 小文字の Method は受理されるべき
      scheme_uppercase.rs       # 大文字の Scheme は受理されるべき
```

- 大文字の HeaderName / Scheme、小文字の Method は RFC 上正当な入力であるため compile-pass とする
- CR / LF / NUL 注入や区切り文字混入は compile-fail とする
- ファイル名は `{type}_{error_kind}.rs` の命名規則

### ランナー実装

```rust
// tests/trybuild.rs
#[test]
fn compile_fail_construct_time_validation() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/trybuild/compile_fail/*.rs");
    t.pass("tests/trybuild/compile_pass/*.rs");
}
```

### コンパイル失敗ソースの例

```rust
// tests/trybuild/compile_fail/header_name_crlf.rs
use shiguredo_http11::HeaderName;

const _BAD: HeaderName = HeaderName::from_static(b"host\r\nX-Inject: evil");

fn main() {}
```

```rust
// tests/trybuild/compile_fail/method_cr.rs
use shiguredo_http11::Method;

const _BAD: Method = Method::from_static(b"GET\r");

fn main() {}
```

```rust
// tests/trybuild/compile_fail/scheme_digit_start.rs
use shiguredo_http11::Scheme;

const _BAD: Scheme = Scheme::from_static(b"3http");

fn main() {}
```

### コンパイル通過ソースの例

```rust
// tests/trybuild/compile_pass/header_name_uppercase.rs
use shiguredo_http11::HeaderName;

// 大文字のヘッダー名は RFC 9110 Section 5.1 上正当 (field-name = token, tchar に ALPHA を含む)
const _OK: HeaderName = HeaderName::from_static(b"Host");

fn main() {}
```

```rust
// tests/trybuild/compile_pass/method_lowercase.rs
use shiguredo_http11::Method;

// 小文字を含むメソッドは RFC 9110 Section 9.1 上正当 (method = token, tchar に ALPHA を含む)
const _OK: Method = Method::from_static(b"get");

fn main() {}
```

```rust
// tests/trybuild/compile_pass/scheme_uppercase.rs
use shiguredo_http11::Scheme;

// 大文字のスキームは RFC 3986 Section 3.1 "should accept uppercase" に従い受理する
const _OK: Scheme = Scheme::from_static(b"HTTPS");

fn main() {}
```

### `.stderr` の生成と再現性

`.stderr` の内容は rustc のバージョンに依存する。`rust-toolchain.toml` は `channel = "stable"` だが
CI 環境で同じバージョンが使われる限り再現性がある。将来的に rustc バージョンが上がり
`.stderr` が不一致になった場合は `TRYBUILD=overwrite cargo test --test trybuild`
で再生成する。

`panic!` 由来の出力を安定させるため、`.stderr` 生成時は `RUST_BACKTRACE=0` を設定する。
`panic!` のメッセージ部分（const fn 内で明示的に指定する文字列）が主な照合対象となる。

### Makefile への組み込み

```makefile
.PHONY: test compile-fail

test:
	cargo test --workspace

compile-fail:
	cargo test --test trybuild
```

既存 `test` ターゲットでも `cargo test --workspace` の一部として実行される。
個別に走らせたい場合のために `compile-fail` ターゲットも用意する。

## 完了条件

- `Cargo.toml` の `[dev-dependencies]` に `trybuild = "1.0"` が追加されている
- `tests/trybuild.rs` ランナーが存在する
- 各 `const fn from_static` API について、発生しうるエラー種別を最低 1 ケースずつカバーしている:

  | 型 | compile-fail ケース | compile-pass ケース |
  |---|---|---|
  | `HeaderName` | 空 / CRLF 注入 / NUL / コロン含有 / 空白 | 大文字 |
  | `Method` | 空 / CR / LF | 小文字 |
  | `Scheme` | 空 / 数字開始 / コロン含有 | 大文字 |

- `cargo test --test trybuild` で全ケースが期待通り pass する
  - `compile_fail/` 内のファイル: コンパイルに失敗すること (= テストとしては成功)
  - `compile_pass/` 内のファイル: コンパイルに成功すること (= テストとしては成功)
- `.stderr` の期待値が生成され、内容が確認済みであること（エラーメッセージが利用者にとって意味があるか）
- `Makefile` に `compile-fail` ターゲットが追加されている（`.PHONY` 行も更新）
- `CHANGES.md` の `## develop` の `### misc` に `[ADD]` エントリが追加されている

## 解決方法

実装順:

1. `Cargo.toml` に `[dev-dependencies]` セクションを新設し `trybuild = "1.0"` を追加
2. `tests/trybuild.rs` ランナーを新設
3. `tests/trybuild/compile_fail/` と `tests/trybuild/compile_pass/` ディレクトリを作成
4. 各ケースの `.rs` ファイルを作成
5. `RUST_BACKTRACE=0 TRYBUILD=overwrite cargo test --test trybuild` で `.stderr` を生成
6. 生成された `.stderr` を内容確認
7. `Makefile` に `.PHONY` 行と `compile-fail` ターゲットを追加
8. `CHANGES.md` に追記

## 関連

- 0091（HeaderName / Method / Scheme 導入）に依存
- 0091 完了後に着手する
