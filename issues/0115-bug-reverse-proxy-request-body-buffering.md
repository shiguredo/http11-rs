# リバースプロキシがリクエストボディを一括バッファリング

- Priority: High
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-reverse-proxy-request-body-streaming
- Polished: 2026-06-13

## 目的

`examples/http11_reverse_proxy` のリクエストボディ転送を一括バッファリングからストリーミングに変更し、大容量アップロード時のメモリ消費と TTFB 悪化を防ぐ。

## 優先度根拠

High とする。ルート `README.md` の `http11_reverse_proxy` 機能説明では「ストリーミング転送」「chunked / content-length / close-delimited 対応」と謳われているが、リクエスト側は `Vec` に全ボディを貯めてから上流へ送信しており、実際にはストリーミングではない。大容量ボディやストリーミングクライアントに対して重大な問題となる。

## 現状

`examples/http11_reverse_proxy/src/main.rs:596-637` で `request_body = Vec::new()` に対し、`decoder.peek_body()` / `consume_body()` で全ボディを蓄積してから `upstream_request.body(request_body)` している。

その後 `examples/http11_reverse_proxy/src/main.rs:690-693` で接続プールから上流接続を取得し、`stream_response_on_connection` 内の `request.encode()` (`src/encoder.rs:822` 相当) でヘッダーとボディを一括エンコードして送信している。これにより、上流接続確立前に下流から全ボディを読み切る必要があり、メモリと TTFB の両方で不利になっている。

## 設計方針

1. 上流接続確立後、下流ボディを読みながら上流に即座に書き出すパイプライン処理に変更する。
2. `BodyKind` に応じた終端判定を正しく行う。リクエストメッセージでは `BodyKind::ContentLength` / `BodyKind::Chunked` / `BodyKind::None` の 3 経路のみが存在し、`BodyKind::CloseDelimited` は存在しない (RFC 9112 Section 6.3 item 7: request message body length is zero unless framed by Content-Length or chunked)。
3. リクエストのフレーミングは元の `BodyKind` を維持する。元が `ContentLength` なら `Content-Length` ヘッダーを保持したまま固定長で流し、元が `Chunked` なら `Transfer-Encoding: chunked` を維持して chunk 単位で流す。`BodyKind::None` ではボディを送信しない。
4. `Request::encode()` の代わりに `Request::encode_headers()` (`src/encoder.rs:1078`、内部で `encode_request_headers` 関数 `src/encoder.rs:915` を呼び出す) を使い、ヘッダー送信後に `encode_chunk` 等でボディをストリーミング送信する。レスポンス側の `stream_response_on_connection` (`examples/http11_reverse_proxy/src/main.rs:940-1010`) と同じパターンを適用する。
5. ただし、`encode_request_headers` は `Content-Length` ヘッダー値と `Request::body_bytes()` の長さが一致しない場合に `EncodeError::ContentLengthMismatch` を返す (`src/encoder.rs:936-947`)。ストリーミング時に `Content-Length` を引き継ぐ場合は、`Request` 側に「ボディ長は後から検証する」モードを追加するか、`Content-Length` を除去して `Transfer-Encoding: chunked` に変換するか、あるいは `encode_request_headers` にストリーミング用のオプションを追加する必要がある。いずれの方針を採用するかは実装時に判断する。
6. 上流への書き込みは `BufWriter` でバッファリングしつつ、chunk 境界や固定長の区切りで適宜 `flush()` する。
7. エラー発生時は下流・上流両方向の接続を適切にクローズし、不完全なリクエストボディが上流に到達しないようにする。

## 完了条件

- リクエストボディが `Vec` に全量貯められないこと。
- `BodyKind::ContentLength` / `BodyKind::Chunked` / `BodyKind::None` の各経路でストリーミング転送が機能すること。
- `RequestDecoder` の `DecoderLimits::max_body_size` を 100 MiB 以上に設定し、ストリーミング転送中に総量上限で誤って拒否されないようにすること。
- 大容量アップロード時のメモリ使用量が大幅に削減されること (例: 100 MiB ボディでもヒープ使用量が読み取りバッファ程度に抑えられる)。
- 下流クライアントが途中で切断した場合、上流への不完全なボディ送信が行われないこと。
- 上流接続の取得に失敗した場合、下流クライアントに適切なエラーレスポンス (502 Bad Gateway 等) が返されること。
- `examples/http11_reverse_proxy` のテストが追加または更新され、chunked / content-length / 大容量ボディの転送を検証すること。
- `CHANGES.md` に `[FIX]` エントリが追加されること。

## 解決方法

- `handle_client` のリクエストボディ読み出し部分を、上流への書き込みと連動させる。
- 上流接続を確立してからボディ転送を開始するようフローを変更する。`upstream_request` は `body()` を使わず `without_body()` のままにしておき、ヘッダーは `encode_request_headers` でエンコードする。
- `BodyKind::ContentLength` の場合:
  - 元の `Content-Length` 値を `upstream_request` に引き継ぐ (`examples/http11_reverse_proxy/src/main.rs:665` で除外している部分を条件付きで保持)。
  - `encode_request_headers` の `ContentLengthMismatch` 検証を回避するため、以下のいずれかを採用する:
    - (a) `Request` / `encode_request_headers` に「ボディ長は後から検証する」ストリーミングモードを追加する。
    - (b) `Content-Length` を除去し、`Transfer-Encoding: chunked` に変換して転送する。
  - `decoder.peek_body()` / `consume_body()` で得たバイト列を上流へ直接 `write_all` する。
  - 規定バイト数に達したら終端とする。
- `BodyKind::Chunked` の場合:
  - 元の `Transfer-Encoding: chunked` を引き継ぐ。
  - `decoder.peek_body()` で得たチャンクデータを `encode_chunk` でフレーミングして上流へ送信する。
  - `BodyProgress::Complete { trailers }` 時に終端チャンク (`0\r\n` + trailers + `\r\n`) を送信する。
- `BodyKind::None` の場合:
  - ボディを送信せず、ヘッダーのみを上流へ送信する。
- テストでは nginx 等の実サーバーを upstream に立て、`curl` やプログラムから 100 MiB 級の chunked / content-length ボディを送信し、メモリ使用量と転送結果を検証する。
- `CHANGES.md` に以下のようなエントリを追加する:
  - `[FIX] examples/http11_reverse_proxy のリクエストボディ転送を一括バッファリングからストリーミングに変更する`
