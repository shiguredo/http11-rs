# 0068 fix URI normalize の冪等性を担保する

- Priority: High
- Created: 2026-05-14
- Completed: 2026-05-14
- Model: Opus 4.7
- Branch: feature/fix-uri-normalize-not-idempotent

## 概要

`Uri::parse("/..//YYYYYYYY/#")` のように authority なしで path に `/../` と `//` の連続を含む URI に対し、`normalize` が冪等にならない。`fuzz_uri_resolve` の `assert_eq!(normalized.as_str(), renormalized.as_str(), "normalize should be idempotent")` (`fuzz/fuzz_targets/fuzz_uri_resolve.rs:48-52`) がこの入力で panic する。

最小再現:

```rust
let uri = Uri::parse("/..//YYYYYYYY/#").unwrap();
let n1 = normalize(&uri).unwrap();   // "//YYYYYYYY/#" に化け、再 parse で authority="YYYYYYYY" / path="/" になる
let n2 = normalize(&n1).unwrap();    // "//yyyyyyyy/#" (host が case-normalize される)
assert_eq!(n1.as_str(), n2.as_str()); // panic
```

`fuzz_uri_resolve` 経由の入力例:

```rust
let base = Uri::parse("/..//YYYYYYYY/").unwrap();
let reference = Uri::parse("#").unwrap();         // reference.fragment() == Some("")
let resolved = resolve(&base, &reference).unwrap(); // "/..//YYYYYYYY/#"
let n1 = normalize(&resolved).unwrap();
let n2 = normalize(&n1).unwrap();
assert_eq!(n1.as_str(), n2.as_str());             // panic (同じ理由)
```

## 目的

`build_uri` が「authority なしで path が `"//"` で始まる」文字列を出力する不具合を修正し、`normalize` の冪等性を回復する。この出力は RFC 3986 Section 3.3 が明示的に禁じる形 (後述) であり、再 parse 時に network-path reference (authority 付き) として解釈される構文上のバグである。

## 優先度根拠

High。

- `Uri::resolve` または `Uri::normalize` を経由すれば外部利用者が誰でも到達できる正しさのバグ
- URI 同一性判定 (キャッシュキー、Origin 比較、リダイレクト先検証等) を破る恐れがある
- fuzz target `fuzz_uri_resolve` が現に検出済みで、`make fuzzing` (ローカル) で再現できる

## 現状

`src/uri.rs` の以下の関数が関与している。

- `pub fn resolve` (`src/uri.rs:772`)
- `fn remove_dot_segments` (`src/uri.rs:844`)
- `fn build_uri` (`src/uri.rs:905`)
- `pub fn normalize` (`src/uri.rs:942`)
- `Uri::parse` の authority 検出ロジック (`src/uri.rs:386-410`)

`Uri::parse` は scheme の `:` 直後 (scheme がなければ `pos=0`) で `bytes[pos] == b'/' && bytes[pos + 1] == b'/'` を満たすと authority を抽出する (`src/uri.rs:387`)。scheme の有無に関係なく `"//"` を authority マーカーとして拾うため、`build_uri` が「authority なし + path が `//` 始まり」の文字列を吐くと、再 parse 時に authority が混入する。

### バグの流れ

1. `resolve(base, reference="#")` は RFC 3986 Section 5.2.2 の path-empty 経路 (`if (R.path == "")` 分岐、`refs/rfc3986.txt:1754-1760`、実装は `src/uri.rs:798-808`) に入り、`build_uri(base.scheme()=None, base.authority()=None, base.path()="/..//YYYYYYYY/", base.query()=None, reference.fragment()=Some(""))` を返す。結果は `"/..//YYYYYYYY/#"`。この経路では `remove_dot_segments` は呼ばれない。
2. `normalize` は `remove_dot_segments(uri.path())` を呼ぶ。`uri.path()` は `"/..//YYYYYYYY/"`。
3. `remove_dot_segments` の RFC 3986 Section 5.2.4 アルゴリズム適用:

   | STEP | output | input | 適用規則 |
   |---|---|---|---|
   | 1 | (空) | `/..//YYYYYYYY/` | - |
   | 2 | (空) | `//YYYYYYYY/` | 2C (`/../` → `/`、output pop は "if any" 分岐で no-op) |
   | 3 | `/` | `/YYYYYYYY/` | 2E (leading `/` を消費し次の `/` 直前 = 空 segment を移動) |
   | 4 | `//YYYYYYYY` | `/` | 2E |
   | 5 | `//YYYYYYYY/` | (空) | 2E |

   結果 `"//YYYYYYYY/"`。これは RFC 3986 Section 5.2.4 アルゴリズム通り。
4. `build_uri(None, None, "//YYYYYYYY/", None, Some(""))` は `"//YYYYYYYY/#"` を出力。
5. `Uri::parse("//YYYYYYYY/#")` は `src/uri.rs:387` の条件 (`pos=0`、`bytes[0]==b'/' && bytes[1]==b'/'`) を満たすため authority 抽出に入り、**authority="YYYYYYYY" / path="/" / fragment=""** として解釈される。
6. 2 回目の `normalize` で host が ASCII 小文字化され `"//yyyyyyyy/#"` になり、1 回目と一致しない。

### 本質

`remove_dot_segments` は RFC 3986 Section 5.2.4 通りに `"//YYYYYYYY/"` を返している。問題は `build_uri` (RFC 3986 Section 5.3 Component Recomposition) で「authority なし、path が `"//"` で始まる」文字列を構成してしまう点。

RFC 3986 Section 3.3 はこの形を明示的に禁じている (`refs/rfc3986.txt:1209-1211`):

> If a URI does not contain an authority component, then the path cannot begin with two slash characters ("//").

ABNF (`refs/rfc3986.txt:1220` および `:1226`) でも `path-absolute = "/" [ segment-nz *( "/" segment ) ]` と定義され、ABNF コメント `; begins with "/" but not "//"` で `//` 始まりが構文的に許されないことが明示されている。`relative-ref` の場合 (Section 4.2, `refs/rfc3986.txt:1428-1443`)、`//` 始まりは唯一 `"//" authority path-abempty` (network-path reference) として解釈される。

Section 5.3 の recomposition 擬似コード (`refs/rfc3986.txt:1916-1928`) は `append path to result;` と書くだけで、path が path-absolute 形であることの保証を呼び出し側に委ねている。よって recomposition 側 (`build_uri`) で構文不変条件を回復する必要がある。

## 設計方針

`build_uri` 内で「`authority.is_none()` かつ `path.starts_with("//")`」のとき、path 先頭に `"/."` を挿入してから serialize する。scheme の有無は条件に含めない。scheme=Some, authority=None, path=`"//Y/"` の場合も `build_uri` は `"file://Y/"` を吐き、`Uri::parse` (`src/uri.rs:387`) が scheme の `:` 直後で `"//"` を authority マーカーとして拾うため同じ曖昧化が起きる。

これにより以下が成立する。

- 再 parse しても path-absolute (RFC 3986 Section 3.3 ABNF `path-absolute = "/" [ segment-nz *( "/" segment ) ]`) として解釈される。`"/." + "//YYYYYYYY/"` = `"/.//YYYYYYYY/"` は `"/"` で始まり 2 文字目が `"."` なので `"//"` 始まりではない。`Uri::parse("/.//YYYYYYYY/")` 内で `src/uri.rs:387` の AND 条件 `bytes[pos] == b'/' && bytes[pos + 1] == b'/'` (pos=0) は右辺 `bytes[1] == b'/'` が `bytes[1]==b'.'` で偽になり authority 抽出に入らない。
- 修正後の `n1` の path-string は `"/.//YYYYYYYY/"`。次回 `normalize` 時には `remove_dot_segments("/.//YYYYYYYY/")` の規則 2B (`"/./"` → `"/"`、`refs/rfc3986.txt:1825-1827`、実装は `src/uri.rs:862-865`) で先頭の `/.` が消えて `"//YYYYYYYY/"` を返す。これを `build_uri` が再度受け取り再び `"/."` を prepend するため `"/.//YYYYYYYY/"` に戻る。1 回目と 2 回目で `as_str()` および `authority()` が完全一致するので冪等性が成立する。

### 影響範囲

`build_uri` は `resolve` から 4 経路、`normalize` から 1 経路の計 5 経路で呼ばれる。「authority なし + path が `//` 始まり」が来うるのは `remove_dot_segments` の結果として `//` 始まりが残るときで、`build_uri` 1 箇所で吸収すれば全経路カバーできる。`merge_paths` / `remove_dot_segments` は RFC 3986 Section 5.2.3 / 5.2.4 アルゴリズム準拠を維持し変更しない。

### 設計上の根拠

- 同等の処理は WHATWG URL Standard `https://url.spec.whatwg.org/#url-serializing` (URL serializing アルゴリズム、host が null で path size > 1 かつ path[0] が空のとき `/.` を prepend) でも採用されている。WHATWG URL Standard は Living Standard のため将来変更の可能性あり。
- 代替案 (a) `remove_dot_segments` 内部改変は RFC 3986 Section 5.2.4 の出力契約 (`refs/rfc3986.txt:1812-1814` で「many ways」と実装自由度を認めつつも、output buffer の最終内容として定まる文字列) と異なる結果を返すことになる。(b) `Uri::parse` で `//` 始まり path-only を reject すると `relative-ref` の network-path reference (Section 4.2) として妥当な構文を拒否することになり受信互換性が壊れる。(c) `resolve` / `normalize` の各経路で個別吸収すると 5 経路で重複コードと漏れの温床になる。よって `build_uri` 1 箇所での吸収を採用する。
- RFC 3986 自身は `/.` prepend を明文化していないが、Section 3.3 の構文不変条件を満たしつつ Section 5.2.4 規則 2B で除去できるため、RFC と矛盾せず冪等性も担保できる。将来 RFC 3986bis 等で明文化される可能性あり。

## 完了条件

- [ ] `build_uri` を「`authority.is_none() && path.starts_with("//")` のとき path 先頭に `"/."` を挿入する」よう修正する
- [ ] `tests/test_uri.rs` に以下の単体テストを追加する。assert メッセージは日本語で書く (CLAUDE.md「テストメッセージは全て日本語」):
  - [ ] `test_uri_normalize_idempotent_with_dotdot_double_slash`: `Uri::parse("/..//YYYYYYYY/#")` を 2 回 normalize して `as_str()` 一致、`authority()` が None、`path() == "/.//YYYYYYYY/"` を assert する
  - [ ] `test_uri_normalize_scheme_only_double_slash`: `Uri::parse("file:/..//Y/")` を normalize した結果が `scheme() == Some("file")`、`authority().is_none()`、`path() == "/.//Y/"`、`as_str() == "file:/.//Y/"` であること、および再度 normalize しても同値であること
- [ ] `pbt/tests/prop_uri.rs` に `path_inducing_double_slash` strategy と以下の property を追加する:
  - [ ] `prop_uri_normalize_idempotent`: 任意の `path_inducing_double_slash` 入力で `normalize(normalize(x)).as_str() == normalize(x).as_str()`
  - [ ] `prop_uri_normalize_no_authority_injection`: `Uri::parse(p)` の authority が None のとき `normalize(uri).authority().is_none()` (片方向検証。本バグの本質である「authority なし → 化ける」を直接検証する)
  - [ ] `prop_uri_normalize_path_no_double_slash_without_authority`: `normalize(x).authority().is_none()` のとき `normalize(x).path()` が `"//"` で始まらない (RFC 3986 Section 3.3 の構文不変条件)
- [ ] PBT strategy `path_inducing_double_slash` を新設する。本バグの再現には「`..` segment + 空 segment + 通常 segment」の構造が必要 (例: `/x/..//Y/`、`/..//Y/`)。`prefix segments + ".." + 空 segment + suffix segments` の形を強制的に組み立てる strategy にする (実装案を参照)。既存 `path()` strategy (`pbt/tests/prop_uri.rs:49-62` 付近) は `.` / `..` / 空 segment を除外しており本バグを誘発できないため別途追加する (既存は変更しない)
- [ ] `cargo fuzz run fuzz_uri_resolve -- -max_total_time=30` を実行し新規 crash が出ないこと。`fuzz/fuzz_targets/fuzz_uri_resolve.rs:48-52` の冪等性 assert は本修正で通る想定で、fuzz target 自体は変更しない
- [ ] `make fmt && make clippy && make check && make test` が pass する
- [ ] `CHANGES.md` の `## develop` に `[FIX]` エントリを既存 `[FIX]` 群の末尾 (`### misc` の直前) に追加する

## 実装案

### `build_uri` 修正

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
        // RFC 3986 Section 3.3: authority なし URI の path は "//" で始まれない
        // (path-absolute = "/" [ segment-nz *( "/" segment ) ]、"begins with / but not //")。
        // Section 5.3 (Component Recomposition) はこの不変条件を呼び出し側に委ねているため、
        // recomposition 側で "/." を挿入し path-absolute に収める。
        // 同等の処理は WHATWG URL Standard の URL serializer でも採用されている
        // (Living Standard、将来仕様改訂で明文化される可能性あり)。
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

### 回帰テスト (tests/test_uri.rs)

```rust
#[test]
fn test_uri_normalize_idempotent_with_dotdot_double_slash() {
    let uri = Uri::parse("/..//YYYYYYYY/#").unwrap();
    let n1 = normalize(&uri).unwrap();
    let n2 = normalize(&n1).unwrap();
    assert_eq!(n1.as_str(), n2.as_str(), "normalize は冪等であること");
    assert_eq!(n1.path(), "/.//YYYYYYYY/", "path が path-absolute 形であること");
    assert!(n1.authority().is_none(), "authority が None で保たれること");
}

#[test]
fn test_uri_normalize_scheme_only_double_slash() {
    let uri = Uri::parse("file:/..//Y/").unwrap();
    let n1 = normalize(&uri).unwrap();
    let n2 = normalize(&n1).unwrap();
    assert_eq!(n1.scheme(), Some("file"), "scheme が file で保たれること");
    assert!(n1.authority().is_none(), "scheme 付きでも authority が None で保たれること");
    assert_eq!(n1.path(), "/.//Y/", "path が path-absolute 形であること");
    assert_eq!(n1.as_str(), "file:/.//Y/", "再 parse 可能な canonical 文字列であること");
    assert_eq!(n1.as_str(), n2.as_str(), "scheme 付きでも冪等であること");
}
```

### PBT strategy と property (pbt/tests/prop_uri.rs)

```rust
// 本バグの再現には ".." segment + 空 segment + 通常 segment の構造が必要。
// 既存 path() strategy は "." / ".." / 空 segment を除外しているため別途追加する。
fn path_inducing_double_slash() -> impl Strategy<Value = String> {
    (
        proptest::collection::vec("[a-zA-Z0-9]{1,4}", 0..3),     // 前置セグメント
        proptest::collection::vec(Just("..".to_string()), 1..3), // 連続する .. セグメント
        proptest::collection::vec("[a-zA-Z][a-zA-Z0-9]{0,7}", 0..3), // 後置セグメント
    )
        .prop_map(|(pre, dd, suf)| {
            let mut segs = pre;
            segs.extend(dd);
            segs.push(String::new()); // 空 segment が "//" 連続を作る鍵
            segs.extend(suf);
            format!("/{}", segs.join("/"))
        })
}

proptest! {
    // strategy は必ず "/" 始まりかつ 2 文字目が非 "/" の入力を返すため、
    // Uri::parse 後の authority は常に None。prop_assume! は不要。
    #[test]
    fn prop_uri_normalize_idempotent(p in path_inducing_double_slash()) {
        let uri = Uri::parse(&p).unwrap();
        let n1 = normalize(&uri).unwrap();
        let n2 = normalize(&n1).unwrap();
        prop_assert_eq!(n1.as_str(), n2.as_str(), "normalize は冪等であること");
    }

    #[test]
    fn prop_uri_normalize_no_authority_injection(p in path_inducing_double_slash()) {
        let uri = Uri::parse(&p).unwrap();
        let normalized = normalize(&uri).unwrap();
        prop_assert!(normalized.authority().is_none(), "authority が新規に注入されないこと");
    }

    #[test]
    fn prop_uri_normalize_path_no_double_slash_without_authority(
        p in path_inducing_double_slash()
    ) {
        let uri = Uri::parse(&p).unwrap();
        let normalized = normalize(&uri).unwrap();
        prop_assert!(
            !normalized.path().starts_with("//"),
            "authority なし URI の path は // で始まらない (RFC 3986 Section 3.3)"
        );
    }
}
```

### CHANGES.md エントリ

`## develop` の既存 `[FIX]` 群の末尾 (`### misc` の直前) に以下を追記する:

```
- [FIX] URI の `normalize` で path-only URI が network-path reference に化けて冪等性が破れる不具合を修正する
  - 旧実装は `build_uri` が「authority なし、path が `//` 始まり」の文字列を構成しており、再 parse で authority に化け、再度 normalize すると host が小文字化されて結果が変わっていた (RFC 3986 Section 3.3 違反)。`build_uri` で `authority.is_none() && path.starts_with("//")` のとき path 先頭に `/.` を挿入するように修正する
  - @voluntas
```

## 解決方法

issue 着手後、`fuzz_uri_resolve` を再実行したところ、本 issue が指定した `//` バグの修正後にも別の冪等性違反が連続して検出された。いずれも `build_uri` (RFC 3986 Section 5.3 Component Recomposition) と `normalize` (Section 6.2.2) の不変条件違反であり、本 issue の射程内とみなして同 PR で併せて修正した。

### 修正 1: `build_uri` で `//` 始まり path に `/.` を prepend (本 issue 本来の対応)

- `src/uri.rs` の `build_uri` で `authority.is_none() && path.starts_with("//")` のとき path 先頭に `"/."` を挿入する
- RFC 3986 Section 3.3 ABNF (`path-absolute = "/" [ segment-nz *( "/" segment ) ]`、"begins with / but not //") への準拠

### 修正 2: `build_uri` で scheme なし + 最初の segment が `:` を含む場合 `./` を prepend (追加対応)

- 修正 1 適用後の fuzz で `base="S55"`, `reference="%55:;:/."` を起因とする別 crash を検出した
- resolve 結果 `path="%55:;:/"` を normalize すると `%55` が `U` にデコードされ `path="U:;:/"` となり、`build_uri` 出力 `"U:;:/"` が再 parse 時に `scheme="U"` に誤解釈されていた
- `src/uri.rs` に `first_segment_contains_colon` ヘルパーを追加し、`build_uri` で `scheme.is_none() && first_segment_contains_colon(path)` のとき path 先頭に `"./"` を挿入する
- RFC 3986 Section 4.2 の MUST 規定 (relative-path reference の最初の segment は `:` を含めず、必要なら dot-segment を前置する) への準拠

### 修正 3: `normalize` の処理順を RFC 3986 Section 6.2.2 通りに修正 (追加対応)

- 修正 2 適用後の fuzz で `%2E` (= `.`) を含む複雑な入力で 3 つ目の crash を検出した
- 旧実装は `remove_dot_segments` を先に呼んでから `normalize_percent_encoding` を呼んでおり、encoded dot (`%2E`) が dot-segment 除去をすり抜けたまま decode され、結果に `/./` が残って次回 normalize で除去 → 非冪等になっていた
- `src/uri.rs::normalize` の path 処理順を「`normalize_percent_encoding` → `remove_dot_segments`」に変更
- RFC 3986 Section 6.2.2 規定 (6.2.2.2 Percent-Encoding Normalization → 6.2.2.3 Path Segment Normalization) への準拠

### テスト

- `tests/test_uri.rs` に単体テスト 4 件を追加
  - `test_uri_normalize_idempotent_with_dotdot_double_slash` (`//` 始まり path)
  - `test_uri_normalize_scheme_only_double_slash` (scheme 付きでも同様)
  - `test_uri_normalize_idempotent_with_colon_first_segment` (`:` を含む最初の segment)
  - `test_uri_normalize_idempotent_with_encoded_dot_segment` (`%2E` の処理順)
- `pbt/tests/prop_uri.rs` に strategy 2 種と property 4 件を追加
  - `path_inducing_double_slash` strategy
  - `path_with_colon_first_segment` strategy
  - `prop_uri_normalize_idempotent` / `prop_uri_normalize_no_authority_injection` / `prop_uri_normalize_path_no_double_slash_without_authority` / `prop_uri_normalize_idempotent_with_colon_first_segment`
- `cargo +nightly fuzz run fuzz_uri_resolve -- -max_total_time=60` を実行し crash なし (9,008,174 runs)
- `make fmt && make clippy && make check && make test` がすべて pass
