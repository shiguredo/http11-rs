# 固定長ボディをヘッダーと別に送るためのヘッダーエンコード経路を整備する

- Created: 2026-11-04
- Completed: {YYYY-MM-DD}
- Branch: feature/change-encode-headers-for-separate-body
- Polished: {YYYY-MM-DD}

## 目的

`Content-Length` で長さを宣言した固定長ボディを、ヘッダーとは別に送る経路を `Request` / `Response` の両方で正規に使えるようにする。
別に送る例は、ファイルからのストリーミング、上流からの転送、ヘッダーとボディを `write_vectored` 等で別々に書く場合である。

現状この経路には次の 3 つの問題がある (2026-11-04 に実行して確認した)。

1. `Request` では、ボディを保持せずに `Content-Length: N` 付きのヘッダーだけをエンコードできない
2. `Response` ではできるが、HEAD レスポンス用の `omit_body(true)` を流用する必要がある
3. ボディを保持している場合でも、`encode_headers()` は `Content-Length` を自動付与しない (`encode()` は付与する)

1 と 2 は、大きいボディや受信中のボディをメモリに載せずに送る用途を塞いでいる。
3 は、`encode()` が毎回ボディを出力 `Vec` へコピーするのを避ける唯一の経路 (ヘッダーとボディを別々に書く) を使いにくくしている。
また 1 と 2 の非対称は、CODEBASE.md の「`Request` / `Response` / `RequestHead` / `ResponseHead` 間で API の一貫性を保つこと」に反する。

## 現状

### ボディなし + Content-Length がエラーになる

`src/encoder.rs` の `encode_request_headers` / `encode_response_headers` は、`Content-Length` ヘッダーの値と `body` の長さが一致することを検証する。
このとき `body == None` を長さ 0 として比較する。

```rust
let body_length = request.body_bytes().map(<[u8]>::len).unwrap_or(0) as u64;
if header_value != body_length {
    return Err(EncodeError::ContentLengthMismatch {
        header_value,
        body_length,
    });
}
```

実行結果:

| 入力 | `encode_headers()` の結果 |
| --- | --- |
| `Request` (PUT) + `Content-Length: 1000` + body なし | `Err(ContentLengthMismatch { header_value: 1000, body_length: 0 })` |
| `Response` (200) + `Content-Length: 1000` + body なし | `Err(ContentLengthMismatch { header_value: 1000, body_length: 0 })` |
| 上記の `Response` + `omit_body(true)` | `Ok("HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\n")` |

- `Request` には `omit_body` に相当する手段がなく、CONNECT 以外では回避できない (CONNECT は `encode_request_headers` が検証自体をスキップする)
- `Response` の回避策は `omit_body(true)` である。お手本の `examples/http11_reverse_proxy` の `stream_response_on_connection` もこれを使っており、コメントで「ヘッダー先行送信パターン」と呼んでいる
- `omit_body` は HEAD レスポンス (RFC 9110 Section 9.3.2) でメッセージボディを送らないためのフラグであり、ボディを別に送るという意味は持たない

### encode_headers() が Content-Length を自動付与しない

`encode_request` / `encode_response` は、`body == Some` かつ `Content-Length` / `Transfer-Encoding` が未指定なら `Content-Length` を自動付与する。
一方 `encode_request_headers` / `encode_response_headers` は付与しない。

| 入力 | 結果 |
| --- | --- |
| `Response` (200) + body 5 バイト + `Content-Length` 未指定、`encode()` | `HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello` |
| 同じ入力、`encode_headers()` | `HTTP/1.1 200 OK\r\n\r\n` |

後者に続けてボディを書くと、受信側はメッセージ長を「サーバーが接続を閉じるまで」と解釈する (RFC 9112 Section 6.3 の 8 項)。

### encode() はボディを毎回コピーする

`encode_request` / `encode_response` は毎回新しい `Vec` を確保し、`buf.extend_from_slice(body)` でボディを複製する。
1 MiB のボディで、出力中のボディ領域が `Response` の保持するボディとは別のアドレスであることを確認した。
ヘッダーとボディを別々に書けばこのコピーは不要だが、前節の理由でその経路は使いにくい。

コピーの性能影響は計測していない。本 issue は `encode()` 自体からコピーを除くことを目的としない。

## 設計方針

`encode_request_headers` / `encode_response_headers` を「ボディは呼び出し側が別に送る」ことを前提とする API として定義し直し、`Request` / `Response` で同じ規則にする。

1. `body == None` のときは、`Content-Length` とボディ長の整合性検証を行わない (ボディはこの値の外にあるとみなす)
   - `Content-Length` の ABNF 検証 (`validate_content_length_headers`)、Transfer-Encoding と Content-Length の競合、1xx / 204 / 205 の制約は従来どおり行う
   - 宣言した長さと実際に送るバイト数を一致させるのは呼び出し側の責務とし、doc コメントに明記する
   - `encode()` は従来どおりで、`body == None` + `Content-Length: N` (N != 0) は `ContentLengthMismatch` のままとする
2. `body == Some` のときは、従来どおり整合性を検証し、`encode()` と同じ条件で `Content-Length` を自動付与する
   - レスポンスは `should_auto_emit_content_length_for_response` を `encode_response` と共有し、判定がずれないようにする
3. 1 により、`encode_response_headers` でボディを別に送るために `omit_body` を使う必要がなくなる。`examples/http11_reverse_proxy` の `set_omit_body(true)` を外す

代替案として、ボディ長だけを持つ表現 (例: `body` を「なし / バイト列 / 長さのみ」の enum にする) を導入する案がある。
意図を型で表せるが、`Request` / `Response` / decoder / PBT への影響が大きい。上記の方針で不足がある場合に比較する。

### 関連 issue

- `issues/pending/0018-change-move-omit-body-to-encoder-flag.md` は `omit_body` をエンコーダー側へ移す提案である
- 本 issue で「ボディを別に送る」用途から `omit_body` を外すと、`omit_body` の用途は HEAD レスポンスだけになる。0018 とは依存関係はなく、どちらを先に進めてもよい

## 完了条件

- `Request` (PUT 等) + `Content-Length: N` + body なしで `encode_headers()` が成功し、`Content-Length: N` を含むヘッダーを返す
- `Response` + `Content-Length: N` + body なしで、`omit_body` を使わずに `encode_headers()` が成功する
- `body == Some` のとき、`encode_headers()` の出力が `encode()` の出力の先頭 (ヘッダー終端の空行まで) とバイト単位で一致する。これを PBT で検証する
- `encode()` では `body == None` + `Content-Length` 不一致が従来どおり `ContentLengthMismatch` になる
- `examples/http11_reverse_proxy` から `set_omit_body(true)` の流用がなくなっている
- `encode_request_headers` / `encode_response_headers` の doc コメントに、ボディを別に送る場合の責務分担が書かれている
- `make fmt && make clippy && make check && make test` がすべて成功する
- `CHANGES.md` の `## develop` に `[CHANGE]` を追記する
