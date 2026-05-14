# 0068 fix URI normalize の冪等性を担保する

- Priority: High
- Created: 2026-05-14
- Model: Opus 4.7
- Branch: feature/fix-uri-normalize-not-idempotent

## 概要

`fuzz_uri_resolve` で以下の入力に対し `normalize should be idempotent` の assert が失敗する。

```text
base      = "/..//YYYYYYYY/"
reference = "#"
```

クラッシュ artifact: `fuzz/artifacts/fuzz_uri_resolve/crash-f510761537720bd0c3f9d041a4d4868eb532eed4`

最小再現コード:

```rust
let base = Uri::parse("/..//YYYYYYYY/").unwrap();
let reference = Uri::parse("#").unwrap();
let resolved = resolve(&base, &reference).unwrap();   // "/..//YYYYYYYY/#"
let n1 = normalize(&resolved).unwrap();                // "//YYYYYYYY/#"
let n2 = normalize(&n1).unwrap();                      // "//yyyyyyyy/#"  ← 冪等でない
assert_eq!(n1.as_str(), n2.as_str());                  // panic
```

## 目的

`normalize` の冪等性 (RFC 3986 Section 6.2.2 由来) を担保することと、それより根の深い問題として「authority なし URI を serialize したときに network-path reference に化ける曖昧化」を防止する。後者はパス比較・同一オリジン判定など URI 比較ロジック全般を破壊するため、セキュリティ的にも避けるべき性質である。

## 優先度根拠

High。

- `Uri::parse` を経由すれば誰でも到達できる正しさのバグである
- path-only URI が再 parse 時に authority 付き URI に化けることで、host が混入し case-insensitive 化されて文字列が変わる。URI 同一性判定 (例: キャッシュキー、Origin 比較、リダイレクト先検証など) を破る恐れがある
- fuzz が現に検出済みで、CI での fuzz 実行が継続的に失敗する状態にある

## 現状

`src/uri.rs` の以下の関数が関与している。

- `pub fn resolve` (`src/uri.rs:772`)
- `fn remove_dot_segments` (`src/uri.rs:844`)
- `fn build_uri` (`src/uri.rs:905`)
- `pub fn normalize` (`src/uri.rs:942`)

### バグの流れ

1. `resolve(base, reference="#")` は `reference.path()` が空なので `build_uri(None, None, "/..//YYYYYYYY/", None, Some("#"))` を返す。結果: `"/..//YYYYYYYY/#"`。
2. `normalize` は `remove_dot_segments(uri.path())` を呼ぶ。`uri.path()` は `"/..//YYYYYYYY/"`。
3. `remove_dot_segments` の処理 (RFC 3986 Section 5.2.4 ベース):
   - `i=0`: `"/../"` にマッチ。`i += 3` (= 3) し `output.pop()` (空)。
   - `i=3`: `path[3..] = "//YYYYYYYY/"`。E ブランチに入り `bytes[3]='/'` なので `i += 1`、直後 `bytes[4]='/'` で内側ループ終了。`output.push("/")`。
   - `i=4`: 同様に E ブランチで `output.push("/YYYYYYYY")`。
   - `i=13`: 末尾 `/` を `output.push("/")`。
   - `output.concat() = "//YYYYYYYY/"`
4. `build_uri(None, None, "//YYYYYYYY/", None, Some("#"))` → `"//YYYYYYYY/#"`。これを `Uri::parse` すると `"//"` 始まりなので **authority = "YYYYYYYY"、path = "/"** として解釈される。
5. 2 回目の `normalize` で host が ASCII 小文字化され `"//yyyyyyyy/#"` になり、初回と一致しない。

### 本質

`remove_dot_segments` 自体は RFC 3986 Section 5.2.4 のアルゴリズムどおりの結果 `"//YYYYYYYY/"` を返している。問題は **authority なし URI を `build_uri` で結合するとき、path が `"//"` で始まると再 parse で認証情報に化ける** という serialize 側の曖昧化である。RFC 3986 はこの状況を URI レベルでは禁じておらず、各実装で曖昧化回避をする必要がある。WHATWG URL、Python `urllib.parse`、Go `net/url` などはいずれも `"/."` を path 先頭に prepend する慣行で対応している。

## 設計方針

`build_uri` (またはそれを呼ぶ各経路) で「scheme/authority がいずれも `None` かつ path が `"//"` で始まる」場合は path 先頭に `"/."` を挿入してから serialize する。これにより:

- 再 parse しても path-absolute (`"/."` + `"//YYYYYYYY/"` = `"/.//YYYYYYYY/"`) として解釈され、authority に化けない
- 次回 `normalize` 時には `remove_dot_segments` の B 規則 (`"/./"` → `"/"`) で `"/."` が消えた後に再び `"//"` 始まりとなり、再度 `"/."` が prepend されるので **冪等性も保たれる** (同じ正規形に収束する)

scheme なし / authority なし以外の経路では曖昧化は起きないため、対策は条件付きで十分。

### 補足: scheme あり authority なし (`"file:..."` 等) の扱い

`"file:/..//Y/"` のようなケースも path が `"//"` 始まりに正規化されると **path-rootless / network-path reference の境界** で曖昧になる可能性があるため、対処範囲を「`authority` が `None` で path が `"//"` で始まる場合」に揃える方針とする。scheme の有無で分岐しないほうが将来 `file:` `urn:` 等を扱う際にも安全。

### 修正範囲

- `fn build_uri` を修正し、`authority.is_none() && path.starts_with("//")` のとき path 先頭に `"/."` を挿入する
- それ以外の関数 (`resolve` / `normalize` / `remove_dot_segments`) はロジック変更しない
- `remove_dot_segments` のアルゴリズムは RFC 3986 Section 5.2.4 準拠を維持する

## 完了条件

- [ ] 本 issue の最小再現入力 (`base="/..//YYYYYYYY/"`, `reference="#"`) で `normalize` が冪等になる
- [ ] `fuzz_uri_resolve` のクラッシュ artifact (`crash-f510761537720bd0c3f9d041a4d4868eb532eed4`) で fuzz が pass する
- [ ] PBT (`pbt/tests/prop_uri.rs`) に「`normalize` 結果が再 parse 後も同じ component 構成 (authority/path) を保つこと」「`normalize(normalize(x)) == normalize(x)`」を任意 `Uri` で検証する property を追加する
- [ ] 単体テスト (`tests/test_uri.rs` 等) に本 issue の入力を回帰テストとして追加する
- [ ] `cargo fuzz run fuzz_uri_resolve fuzz/artifacts/fuzz_uri_resolve/crash-f510761537720bd0c3f9d041a4d4868eb532eed4` が pass する
- [ ] `make fmt && make clippy && make check && make test` が pass する
- [ ] `CHANGES.md` の `## develop` に `[FIX]` エントリを追加する

## 解決方法

### 1. `build_uri` 修正

```rust
fn build_uri(
    scheme: Option<&str>,
    authority: Option<&str>,
    path: &str,
    query: Option<&str>,
    fragment: Option<&str>,
) -> String {
    let mut result = String::new();

    if let Some(s) = scheme {
        result.push_str(s);
        result.push(':');
    }

    if let Some(a) = authority {
        result.push_str("//");
        result.push_str(a);
    } else if path.starts_with("//") {
        // RFC 3986 Section 5.3 に明示的記述はないが、authority なし URI で path が
        // "//" で始まると再 parse 時に network-path reference (authority 付き) として
        // 解釈されてしまう。これを防ぐため "/." を path 先頭に挿入する。
        // この対処は WHATWG URL、Python urllib、Go net/url 等の慣行に倣う。
        result.push_str("/.");
    }

    result.push_str(path);

    if let Some(q) = query {
        result.push('?');
        result.push_str(q);
    }

    if let Some(f) = fragment {
        result.push('#');
        result.push_str(f);
    }

    result
}
```

### 2. 回帰テスト追加

`tests/test_uri.rs` に以下を追加:

```rust
#[test]
fn normalize_は_path_先頭の_dot_dot_と_double_slash_の組合せでも冪等() {
    let resolved = Uri::parse("/..//YYYYYYYY/#").unwrap();
    let n1 = normalize(&resolved).unwrap();
    let n2 = normalize(&n1).unwrap();
    assert_eq!(n1.as_str(), n2.as_str());
    // authority に化けないこと
    assert!(n1.authority().is_none());
}
```

### 3. PBT 追加

`pbt/tests/prop_uri.rs` に以下の property を追加:

- `normalize(normalize(x)).as_str() == normalize(x).as_str()` (冪等性)
- `normalize(x)` の component 構成 (authority の有無) が `x` と一致すること (path → authority 化けの防止)

## CHANGES.md

`## develop` の先頭に以下を追記:

```
- [FIX] URI の `normalize` が path-only URI を network-path reference に化けさせ冪等性を破る不具合を修正する
  - 例: `/..//YYYYYYYY/#` を normalize すると `//YYYYYYYY/#` (authority 付き URI に化ける) になり、再 normalize で host が小文字化されて結果が変わっていた
  - `build_uri` で authority なしかつ path が `//` 始まりの場合に `/.` を prepend する対処を追加する
  - @voluntas
```
