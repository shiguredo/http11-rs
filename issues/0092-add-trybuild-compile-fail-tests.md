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

## 優先度根拠

Medium。0091 と同じ。「コンパイル時検査が壊れた」ことをサイレントに見逃さないための
リグレッション防止層であり、0091 の価値を CI で維持するために必須。0091 完了後すぐに着手する。

## 現状

http11 には現在 `const fn` ベースの構築 API が無いため、本テストは存在しない。

## 設計方針

### 依存追加

```toml
# Cargo.toml
[dev-dependencies]
trybuild = "1"
```

`trybuild` は `dev-dependencies` 限定でライブラリ本体ビルドには影響しない (依存 0 ポリシーを
維持できる)。

### テスト配置

```
tests/
  trybuild.rs                                # ランナー
  trybuild/
    header_name_uppercase.rs
    header_name_uppercase.stderr
    header_name_with_crlf.rs
    header_name_with_crlf.stderr
    header_name_empty.rs
    header_name_empty.stderr
    method_lowercase.rs
    method_lowercase.stderr
    method_with_space.rs
    method_with_space.stderr
    scheme_starting_with_digit.rs
    scheme_starting_with_digit.stderr
    scheme_with_uppercase.rs                 # 注: scheme は大文字小文字許容なので別ケース
    scheme_empty.rs
    scheme_empty.stderr
```

### ランナー実装

```rust
// tests/trybuild.rs
#[test]
fn compile_fail_construct_time_validation() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/trybuild/*.rs");
}
```

### コンパイル失敗ソースの例

```rust
// tests/trybuild/header_name_uppercase.rs
use shiguredo_http11::HeaderName;

const _BAD: HeaderName = HeaderName::from_static(b"Host");

fn main() {}
```

```rust
// tests/trybuild/method_lowercase.rs
use shiguredo_http11::Method;

const _BAD: Method = Method::from_static(b"get");

fn main() {}
```

### rustc バージョン依存性

`.stderr` の内容は rustc のバージョンに依存して微変する。本リポジトリは
`rust-toolchain.toml` でバージョン固定 (MSRV 1.88) しているため、CI で同じ
バージョンが使われる限り再現性がある。

複数バージョンでテストする場合は `TRYBUILD=overwrite cargo test` で `.stderr`
を再生成する運用にする。

### Makefile への組み込み

```makefile
# Makefile
test:
	cargo test --workspace

compile-fail:
	cargo test --test trybuild
```

既存 `test` ターゲットでも `cargo test --workspace` の一部として実行される。
個別に走らせたい場合のために `compile-fail` ターゲットも用意する。

## 完了条件

- `Cargo.toml` の `[dev-dependencies]` に `trybuild = "1"` が追加されている
- `tests/trybuild.rs` ランナーが存在する
- 全 `const fn from_static` API について少なくとも 1 件以上の compile-fail ケースが存在する:
  - `HeaderName::from_static`: 大文字 / CRLF / 空 / token 文字以外
  - `Method::from_static`: 小文字 / 空 / 制御文字 / SP
  - `Scheme::from_static`: 数字開始 / 空 / 不正文字
- `cargo test --test trybuild` で全ケースが期待通り fail (テスト的には pass) する
- `.stderr` の期待値が `rust-toolchain.toml` の固定バージョンと一致している
- `Makefile` に `compile-fail` ターゲットが追加されている
- `CHANGES.md` の `## develop` の `### misc` に `[ADD]` エントリが追加されている

## 解決方法

実装順:

1. `Cargo.toml` に `trybuild` を追加
2. `tests/trybuild.rs` を新設
3. 各構築時検査型ごとに失敗ケース `.rs` ファイルを作成
4. `cargo test --test trybuild` を `TRYBUILD=overwrite` 付きで実行し `.stderr` を生成
5. 生成された `.stderr` を内容確認 (エラーメッセージが利用者にとって意味があるか)
6. `Makefile` にターゲット追加
7. `CHANGES.md` に追記

## 関連

- 0091 (HeaderName / Method / Scheme 導入) に依存
