# Request / Response にボディを含む所有値を取り出す消費メソッドを追加する

- Created: 2026-11-04
- Completed: {YYYY-MM-DD}
- Branch: feature/add-request-response-consuming-methods
- Polished: {YYYY-MM-DD}

## 目的

`Request` / `Response` が所有しているボディ (`Vec<u8>`) やヘッダーを、コピーせずに取り出せるようにする。

- `RequestDecoder::decode` / `ResponseDecoder::decode` は、デコード済みのボディを `core::mem::take` で取り出し、`Request::from_raw_parts` / `Response::from_raw_parts` にムーブして値を構築している
- しかし利用者が使えるのは借用の `body_bytes() -> Option<&[u8]>` だけである。所有した `Vec<u8>` が必要な場面 (`String::from_utf8` に渡す、別のバッファや API へ引き渡す等) では `to_vec()` によるコピーが避けられない
- `RequestHead` / `ResponseHead` には `into_*()` / `into_parts()` があるが、`Request` / `Response` には消費メソッドが 1 つもない。CODEBASE.md の「`Request` / `Response` / `RequestHead` / `ResponseHead` 間で API の一貫性を保つこと」に反する

## 現状

- `src/request.rs` の `Request` と `src/response.rs` の `Response` が公開するボディ関連 API は次のとおりである
  - ビルダー: `body` / `without_body`
  - mutator: `set_body` / `clear_body`
  - 借用 getter: `body_bytes`
- `into_body` / `take_body` / `into_parts` 等の消費メソッドはない。`From` 等の変換 trait の実装もなく、実装している trait は `HttpHead` だけである
- `src/decoder/head.rs` の `RequestHead` / `ResponseHead` の消費メソッドは、ストリーミングデコーダーの利用者が再アロケーションを避けられるように追加された (`issues/closed/0106-add-consuming-methods-to-head-types.md`)。このとき `Request` / `Response` は対象外で、意図的に見送ったという記録はない

## 設計方針

`RequestHead` / `ResponseHead` の命名と戻り値の形にそろえ、ボディを加える。

`Request`:

- `into_method(self) -> Method`
- `into_uri(self) -> String`
- `into_version(self) -> String`
- `into_headers(self) -> Vec<(HeaderName, String)>`
- `into_body(self) -> Option<Vec<u8>>`
- `into_parts(self) -> (Method, String, String, Vec<(HeaderName, String)>, Option<Vec<u8>>)`

`Response`:

- `into_version(self) -> String`
- `into_reason_phrase(self) -> String`
- `into_headers(self) -> Vec<(HeaderName, String)>`
- `into_body(self) -> Option<Vec<u8>>`
- `into_parts(self) -> (String, u16, String, Vec<(HeaderName, String)>, Option<Vec<u8>>)`

方針の補足:

- `body` は `None` (ボディなし) と `Some(vec![])` (明示的な空ボディ) を区別しているため、`Option<Vec<u8>>` のまま返す
- `into_parts` のタプルは、`RequestHead::into_parts` / `ResponseHead::into_parts` の末尾にボディを足した形にする
- `Response` の `omit_body` は送信制御のフラグなので `into_parts` に含めない (`issues/pending/0018-change-move-omit-body-to-encoder-flag.md` で撤去が検討されている)
- `&mut self` でボディだけを抜く `take_body` は、`RequestHead` / `ResponseHead` との一貫性を優先して本 issue では追加しない
- `RequestHead` / `ResponseHead` と同じく `#[must_use]` を付ける

## 完了条件

- 上記のメソッドが `Request` / `Response` に追加され、doc コメントが書かれている
- `into_body()` が `None` / `Some(vec![])` / `Some(data)` を区別して返すことを単体テストで確認する
- `body(v)` で渡した `Vec` と `into_body()` で取り出した `Vec` のバッファのアドレスが一致する (コピーされない) ことを単体テストで確認する
- `into_parts()` で取り出した値から再構築した `Request` / `Response` が元の値と一致することを PBT で確認する (`Response` の `into_parts()` は `omit_body` を含まないため、`omit_body == false` の値で確認する)
- `make fmt && make clippy && make check && make test` がすべて成功する
- `CHANGES.md` の `## develop` に `[ADD]` を追記する
