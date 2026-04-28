# 0004: Request/Response の body を Option<Vec<u8>> に変更

Created: 2026-04-28
Completed: 2026-04-28
Model: Opus 4.7 (1M context)

## 概要

`Request::body` および `Response::body` を `Vec<u8>` から `Option<Vec<u8>>` に変更し、「ボディなし」と「明示的な空ボディ」を型レベルで区別できるようにする。

## 動機

現状の `body: Vec<u8>` 設計には以下の問題がある。

- `body == Vec::new()` の状態が「ボディを送る意図がない」のか「明示的に空ボディを送る」のか判別できない。
- そのため `encoder.rs` の `Content-Length` 自動付与ロジックは `body.is_empty()` のときに必ず `Content-Length` を省略する実装になっており、`POST` / `PUT` / `PATCH` のように request content に意味を持つメソッドで「空 content + `Content-Length: 0`」を送る正規ルートが存在しない。
- デコーダー側でも、フレーミングのないリクエストとフレーミングはあったが長さ 0 だったリクエストを区別できず、ラウンドトリップで情報が失われる。

RFC 9110 Section 8.6:
> A user agent SHOULD send Content-Length in a request when the method defines a meaning for enclosed content and it is not sending Transfer-Encoding.

「メソッド意味論で content が想定されるか」をライブラリ側で勝手に判定することはせず、呼び出し側が `Some(vec![])` / `None` を明示することで意図を伝える設計に統一する。

## 設計

### 状態と意味

| 状態 | 意味 | エンコーダーの挙動 |
| ---- | ---- | ------------------ |
| `body: None` | ボディを送る意図がない | `Content-Length` を自動付与しない |
| `body: Some(vec![])` | 明示的に空ボディ | `Content-Length: 0` を自動付与 |
| `body: Some(data)` | 通常のボディ | `Content-Length: N` を自動付与 |

### Request / Response の API

- `pub body: Option<Vec<u8>>` に変更する。
- ビルダー `body(Vec<u8>) -> Self` は内部で `Some(...)` をセットする。シグネチャは互換のまま。
- `Request::new` / `Response::new` のデフォルトは `body: None`。

### encoder

- `Content-Length` 自動付与は `body.as_ref()` を見て分岐する。
  - `None` → 何もしない (Transfer-Encoding 等は別途維持)
  - `Some(b)` かつ `Content-Length` / `Transfer-Encoding` 未指定 → `Content-Length: b.len()` を付与
- `Content-Length` ヘッダーが明示されている場合の値整合性検証は `body.as_deref().map(<[u8]>::len).unwrap_or(0)` を基準に行う。`body == None` で `Content-Length` を明示している場合は本数値が 0 として扱われる。
- 205 / HEAD などの既存制約は変更しない。

### decoder

- フレーミング (Content-Length / Transfer-Encoding) が存在せずボディセクションを読まなかったケースは `body: None` で表現する。
- `Content-Length: 0` で明示された空ボディ、空 chunked、close-delimited で 0 バイトのケースは `body: Some(vec![])` で表現する。

### Response::omit_body

`omit_body` は残す。これは「`Content-Length` は表現長として残しつつ message body を送らない」(HEAD レスポンス) のための直交フラグであり、`body` の有無とは別の概念。

## 影響範囲

破壊的変更だが機械的に追従可能:

- `src/request.rs`, `src/response.rs` (フィールド型とビルダー)
- `src/encoder.rs` (Content-Length 自動付与・整合性検証)
- `src/decoder/body.rs`, `src/decoder/request.rs`, `src/decoder/response.rs` (デコーダーの body 出力)
- `examples/http11_client`, `examples/http11_server`, `examples/http11_reverse_proxy`
- `tests/`, `pbt/tests/` 全般

## 検証

- `make fmt && make clippy && make check && make test` をすべて通過させる。
- 既存の単体テスト/PBT が `Option` 化に追従して緑であることを確認する。
- 「`POST` + `Some(vec![])` で `Content-Length: 0` が出ること」「`POST` + `None` で `Content-Length` が出ないこと」をエンコーダーの単体テストに追加する。

## 解決方法

- `src/request.rs` / `src/response.rs` の `body` フィールドを `Vec<u8>` から `Option<Vec<u8>>` に変更し、デフォルト値を `None` に変更した。ビルダー `body(Vec<u8>) -> Self` は内部で `Some(...)` をセットするためシグネチャは互換のまま。
- `src/encoder.rs` の `Content-Length` 自動付与ロジックを `body.as_deref()` ベースに書き換えた。`None` のときは `Content-Length` を付与せず、`Some(b)` のときは `b.len()` を付与する。レスポンス側は `(omit_body, body_len)` の組み合わせで分岐 (`omit_body && Some(0)` は付与しない HEAD ケース、それ以外で `Some(_)` なら付与) する。
- `src/encoder.rs` のレスポンス 205 / `Content-Length` 整合性検証も `body.as_deref().map(<[u8]>::len).unwrap_or(0)` ベースに調整した。
- `src/decoder/request.rs` / `src/decoder/response.rs` の出力を `BodyKind` で分岐: `None` / `Tunnel` は `body = None`、それ以外 (`ContentLength` / `Chunked` / `CloseDelimited`) は `Some(...)` を返すようにした。
- 破壊的変更に追従して `examples/http11_client`、`examples/http11_reverse_proxy`、`examples/http11_server` を `body.as_deref()` 経由に書き換えた。リバースプロキシは元リクエストの `BodyKind::None` を保ったまま upstream に転送する (`upstream_request.body = None`)。
- `tests/test_decode_body.rs`、`tests/test_encoder.rs`、`pbt/tests/prop_request.rs`、`pbt/tests/prop_response.rs`、`pbt/tests/prop_decoder/{body,request,response}.rs` を `Option` ベースの比較に追従させた。
- ラウンドトリップ系 PBT (`prop_response_roundtrip` など) は、`Response::new(...).encode()` を `.body()` を呼ばずに使うと status_has_body 系コードで close-delimited になり `decode()` が EOF 待ちになるため、ステータスごとに `Vec::new()` を明示する形に修正した。
- `tests/test_encoder.rs` に `test_encode_post_with_explicit_empty_body_emits_content_length_zero` / `test_encode_post_without_body_emits_no_content_length` / `test_encode_get_without_body_emits_no_content_length` を追加し、新しい `Some/None` の挙動を単体テストで固定した。
- 検証: `make fmt`、`make clippy` (-D warnings)、`make check`、`make test` をすべて通過した。
