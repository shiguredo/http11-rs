# 0009: get_headers() に allocation しない iterator API を追加する

Created: 2026-04-28
Model: Kimi 2.6 / GPT 5.5 / Composer 2 Fast

## 概要

`get_headers()` が毎回 `Vec<&str>` をアロケートする設計を改善し、allocation しないイテレータを返す API を追加する。`dyn HttpHead` 利用者に対しては破壊的変更。`Request` / `Response` / `RequestHead` / `ResponseHead` の利用者に対してはシグネチャ維持。

## 根拠

現状の `Request::get_headers()` と `Response::get_headers()`（および `HttpHead::get_headers()`）は `Vec<&str>` を返す。これは API / 性能 / no_std + alloc 設計の問題であり、毎回ヒープアロケーションが発生する。

さらに現状の `HttpHead::is_keep_alive()` と `is_chunked()` は内部で `get_headers("Connection")` および `get_headers("Transfer-Encoding")` を呼んでおり、これらのメソッド呼び出しのたびに不要な allocation が積み重なる実害がある。

## 技術的制約

`HttpHead` は公開トレイト（`pub trait`）であり、将来 `dyn HttpHead` を使う可能性があるため、object safety を考慮する必要がある。現状 `dyn HttpHead` は使われていないが、保守的に設計する。

`impl Iterator` を戻り値にするメソッド（RPITIT）は **object safe ではない**。したがって `dyn HttpHead` からは呼び出せない。これを回避するには `Self: Sized` 制約を付ける。

`Self: Sized` 制約付きメソッドは object safe な trait に追加可能だが、`dyn Trait` からは呼び出せない。

## 設計

`Self: Sized` 制約付きの `impl Iterator` を採用する。

```rust
pub trait HttpHead {
    fn version(&self) -> &str;
    fn headers(&self) -> &[(String, String)];

    // 以下は Self: Sized なしで object safe に維持
    fn get_header(&self, name: &str) -> Option<&str> { ... }
    fn has_header(&self, name: &str) -> bool { ... }
    fn connection(&self) -> Option<&str> { ... }
    fn content_length(&self) -> Option<u64> { ... }

    // 以下は Self: Sized 制約付き（dyn HttpHead では使えない）
    fn header_values(&self, name: &str) -> impl Iterator<Item = &str>
    where
        Self: Sized,
    {
        self.headers()
            .iter()
            .filter(move |(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    fn get_headers(&self, name: &str) -> Vec<&str>
    where
        Self: Sized,
    {
        self.header_values(name).collect()
    }

    fn is_keep_alive(&self) -> bool
    where
        Self: Sized,
    {
        let mut has_keep_alive = false;
        for conn in self.header_values("Connection") {
            for token in conn.split(',') {
                let token = token.trim();
                if token.eq_ignore_ascii_case("close") {
                    return false;
                }
                if token.eq_ignore_ascii_case("keep-alive") {
                    has_keep_alive = true;
                }
            }
        }
        if has_keep_alive {
            return true;
        }
        self.version().ends_with("/1.1")
    }

    fn is_chunked(&self) -> bool
    where
        Self: Sized,
    {
        let mut last_token: Option<&str> = None;
        for te in self.header_values("Transfer-Encoding") {
            for token in te.split(',') {
                let token = token.trim();
                if !token.is_empty() {
                    last_token = Some(token);
                }
            }
        }
        last_token.is_some_and(|t| t.eq_ignore_ascii_case("chunked"))
    }
}
```

## 破壊的変更

以下のメソッドに `Self: Sized` 制約が追加されるため、`dyn HttpHead` では使えなくなる。

- `get_headers()`
- `is_keep_alive()`
- `is_chunked()`

`dyn HttpHead` を使っているコードがあればコンパイルエラーになるが、現状コードベースでは `dyn HttpHead` を使っていない。

`get_header()` / `has_header()` / `connection()` / `content_length()` は `Self: Sized` なしで維持されるため、`dyn HttpHead` では引き続き使える。

## 対象ファイルと変更点

### `src/decoder/head.rs`

`HttpHead` トレイトに上記の変更を適用する。

### `src/request.rs` / `src/response.rs`

0007 で `HttpHead` を実装するため、inherent method は不要。ただし `get_header()` 等の inherent method は公開 API 維持のため残す（0007 で `HttpHead` メソッドに委譲する）。

### `src/lib.rs`

`HttpHead` トレイトが既に公開されているため、追加の変更は不要。

## 影響範囲

- `HttpHead` トレイトの `get_headers()` / `is_keep_alive()` / `is_chunked()` に `Self: Sized` 制約が追加される。
- `dyn HttpHead` で上記メソッドを使っていた場合、コンパイルエラーになる（現状使っていない）。
- `header_values()` は新規追加 API。

## 検証

- `make fmt && make clippy && make check && make test` を通す。
- 既存の `get_headers()` 呼び出しがそのまま動作することを確認する。
- `is_keep_alive()` / `is_chunked()` の呼び出しで allocation が減少することを確認（可能であればベンチマーク）。
- `dyn HttpHead` で `get_header()` は使えるが `get_headers()` / `is_keep_alive()` はコンパイルエラーになることを確認する（テストコードで確認）。
- `CHANGES.md` に `[CHANGE]` セクションを追記する（`dyn HttpHead` 利用者への破壊的変更）。
