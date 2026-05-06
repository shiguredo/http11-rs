# 0022: Response::get_headers をイテレータベースに変更する

Created: 2026-05-06
Completed: 2026-05-06
Model: Opus 4.7

## 概要

`Response::get_headers(&self, name: &str) -> Vec<&str>` を `pub fn get_headers(&self, name: &str) -> impl Iterator<Item = &str>` に変更し、戻り値のヒープ確保を排除する。`HttpHead::headers` 経由のアクセスについても、用途に応じたイテレータベース API を追加する。

破壊的変更。`response.get_headers(name)` の戻り値を `Vec<&str>` として扱っている呼び出し側は `.collect::<Vec<_>>()` 等を追加する必要がある。

## 根拠

### 問題 1: `Vec<&str>` を返すたびにヒープ確保

```rust
pub fn get_headers(&self, name: &str) -> Vec<&str> {
    HttpHead::get_headers(self, name)
}
```

戻り値が `Vec<&str>` のため、呼び出すたびにヒープ確保が走る。CLAUDE.md「依存は最小限」「Sans I/O 設計」観点で、避けられるアロケーションは避けるべき。

`impl Iterator<Item = &str>` を返せば:

- 呼び出し側が `.next()` で 1 件だけ取りたい場合 → ヒープ確保ゼロ
- `.collect::<Vec<_>>()` したい場合 → 既存と同じ挙動
- `.find` / `.filter` / `.count` 等の任意の操作が可能

利用側に裁量を譲る方が API として柔軟。

### 問題 2: HttpHead::headers の用途別 API が不在

`HttpHead::headers` は `&[(String, String)]` を返すため、特定ヘッダーの抽出は呼び出し側で `iter().find_map(...)` を書く必要がある。`headers_by_name(name) -> impl Iterator<Item = &str>` のような用途別 API が `HttpHead` トレイトに追加されると、利用側のボイラープレートが減る。

## 対応方針

### src/decoder/head.rs (HttpHead トレイト)

```rust
pub trait HttpHead {
    fn version(&self) -> &HttpVersion;
    fn headers(&self) -> &[(String, String)];

    /// 指定された名前のヘッダー値をすべて返す (case-insensitive)
    fn headers_by_name<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a str> {
        self.headers()
            .iter()
            .filter(move |(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    // 既存の get_headers (Vec<&str> 返却) はデフォルト実装から撤去する
    // get_header (Option<&str> 返却) は既存維持
    // ...
}
```

注: Rust 2024 edition で `impl Trait` をトレイトメソッドの戻り値に使う際の制約 (RPITIT) を確認する。安定化済みのため通常は問題ないが、現行の rustc バージョンと edition を確認する必要がある。

### src/response.rs / src/request.rs

```rust
impl Response {
    pub fn get_headers<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a str> {
        HttpHead::headers_by_name(self, name)
    }
}
```

`Request` 側も同様に変更する。

### tests / pbt / examples

- `response.get_headers(name)` を `Vec<&str>` として使っている箇所を `.collect::<Vec<_>>()` 化、または直接イテレータ操作に書き換える
- `closed/0009-enhance-get-headers-iterator-api.md` で類似の議論があれば参照する (issue 番号から類推して既存の検討内容を確認すること)

注: `closed/0009` は本 issue と同名の可能性が高いので、必ず参照して差分を明確にすること。本 issue は Response 限定の追加対応か、Request も含めた包括対応かを再確認する必要がある。

## CHANGES.md

`## develop` に以下を追加する:

```
- [CHANGE] `Response::get_headers` / `Request::get_headers` の戻り値を `Vec<&str>` から `impl Iterator<Item = &str>` に変更する
  - 戻り値のヒープ確保を排除する
  - 既存呼び出し側で `Vec<&str>` を期待している箇所は `.collect::<Vec<_>>()` に書き換える
  - @voluntas
- [ADD] `HttpHead::headers_by_name` を追加し、ヘッダー名による case-insensitive な抽出をイテレータで提供する
  - @voluntas
```

## 検証方針

- 既存の `get_headers(name)` の呼び出し側が `collect()` 経由で同じ挙動を保つことを単体テスト・PBT で確認
- 新規単体テストで `headers_by_name` が複数値を返すケース (Set-Cookie 等) を確認
- イテレータの遅延評価により `.next()` だけ呼ぶケースでヒープ確保が発生しないことを意図する設計であることを doc に明記

## 受け入れ基準

- `make fmt && make clippy && make check && make test` がすべて成功する
- `Response::get_headers` / `Request::get_headers` の戻り値が `impl Iterator<Item = &str>` になっている
- `HttpHead::headers_by_name` が公開 API として存在する
- 既存テストが新 API に追従して green になる
- `closed/0009` との重複・差分が明確化されている

## 解決方法

対応不要。本 issue が提起する `get_headers()` 戻り値の allocation 問題は、`closed/0009` で `is_keep_alive()` / `is_chunked()` 内部実装の直接走査により既に回避されている。

また `get_headers()` を `impl Iterator` 化すると `HttpHead` トレイトの object safety が破壊され、かつ呼び出し側で `.len()` やインデックスアクセスを使う既存テストがすべて `.collect()` 必須になる。これらの破壊的変更に見合う根拠が本 issue には示されておらず、0009 の設計判断を覆す理由はない。
