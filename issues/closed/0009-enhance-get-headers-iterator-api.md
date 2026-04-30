# 0009: `is_keep_alive()` / `is_chunked()` の内部実装を直接走査に変更して allocation を削減する

Created: 2026-04-28
Completed: 2026-04-30
Model: Kimi 2.6 / GPT 5.5 / Composer 2 Fast

## 概要

`HttpHead::is_keep_alive()` と `is_chunked()` が内部で `get_headers()` を呼んでおり、呼び出しのたびに `Vec<&str>` をアロケートしていた。この不要な allocation を `headers().iter()` で直接走査することで回避する。

`get_headers()` 自体は object safe のまま維持し、既存 API のシグネチャを変更しない。

## 根拠

現状の `HttpHead::is_keep_alive()` と `is_chunked()` は内部で `get_headers("Connection")` および `get_headers("Transfer-Encoding")` を呼んでいる。`get_headers()` は `Vec<&str>` を返すため、これらのメソッド呼び出しのたびに不要なヒープアロケーションが発生する。

`headers()` は `&[(String, String)]` を返す既存のトレイトメソッドであり、`is_keep_alive()` / `is_chunked()` の実装は `headers().iter()` を直接走査すれば allocation なしに同じ結果が得られる。

## 技術的制約

元 issue では `impl Iterator` を戻り値にする `header_values()` メソッドを `Self: Sized` 付きで追加し、`get_headers()` / `is_keep_alive()` / `is_chunked()` も `Self: Sized` にする案を検討した。しかし、これにより `dyn HttpHead` でこれらのメソッドが使えなくなる破壊的変更が生じる。

実際には `Self: Sized` を付ける必要はない。`is_keep_alive()` / `is_chunked()` の内部実装を `headers().iter()` で直接走査するだけで allocation は回避でき、`get_headers()` のシグネチャも維持できる。

## 設計

`HttpHead` トレイトの `is_keep_alive()` と `is_chunked()` の内部実装を変更する。`get_headers()` の呼び出しを `headers().iter()` での直接走査に置き換える。

```rust
fn is_keep_alive(&self) -> bool {
    let mut has_keep_alive = false;
    for (name, value) in self.headers() {
        if !name.eq_ignore_ascii_case("Connection") {
            continue;
        }
        for token in value.split(',') {
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

fn is_chunked(&self) -> bool {
    let mut last_token: Option<&str> = None;
    for (name, value) in self.headers() {
        if !name.eq_ignore_ascii_case("Transfer-Encoding") {
            continue;
        }
        for token in value.split(',') {
            let token = token.trim();
            if !token.is_empty() {
                last_token = Some(token);
            }
        }
    }
    last_token.is_some_and(|t| t.eq_ignore_ascii_case("chunked"))
}
```

## 破壊的変更

なし。`get_headers()` / `is_keep_alive()` / `is_chunked()` のシグネチャは変更しない。

## 対象ファイルと変更点

### `src/decoder/head.rs`

- `is_keep_alive()`: `self.get_headers("Connection")` の呼び出しを `self.headers().iter()` での直接走査に置き換える
- `is_chunked()`: `self.get_headers("Transfer-Encoding")` の呼び出しを `self.headers().iter()` での直接走査に置き換える

### `src/request.rs` / `src/response.rs`

変更なし。0007 で `HttpHead` デフォルト実装に委譲しているため、自動的に追随する。

## 影響範囲

- `HttpHead` トレイトの `is_keep_alive()` / `is_chunked()` の内部実装のみ変更
- `get_headers()` のシグネチャと動作は変更しない
- `dyn HttpHead` の object safety は維持される
- `Request` / `Response` / `RequestHead` / `ResponseHead` は自動的に追随する

## 検証

- `make fmt && make clippy && make check && make test` を通す。
- 既存の `get_headers()` / `is_keep_alive()` / `is_chunked()` の呼び出しがそのまま動作することを確認する。
- `is_keep_alive()` / `is_chunked()` の呼び出しで allocation が減少することを確認する。

## 解決方法

`src/decoder/head.rs` の `is_keep_alive()` と `is_chunked()` の内部実装を、`get_headers()` を経由せず `headers().iter()` で直接走査するように変更した。これにより、呼び出し時の不要な `Vec<&str>` allocation が回避される。`get_headers()` / `is_keep_alive()` / `is_chunked()` のシグネチャは変更せず、object safe を維持する。
