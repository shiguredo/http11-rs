# リバースプロキシがリクエストボディを一括バッファリング

- Priority: High
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-reverse-proxy-request-body-streaming
- Polished: 2026-06-16

## 目的

`examples/http11_reverse_proxy` のリクエストボディ転送を一括バッファリングからストリーミングに変更し、大容量アップロード時のメモリ消費と TTFB 悪化を防ぐ。

本 issue は 0111 (下流接続再利用) → 0114 (close 双方向伝達) → **0115 (リクエストボディストリーミング、本 issue)** の依存順の最終段。0111 / 0114 完了後にマージする。

### 前提となる本体ライブラリ API 拡張 (別 issue)

設計方針 5 で必要となる「`Content-Length` を持つリクエストをストリーミング転送する」ための encoder API 拡張 (`encode_request_headers` にストリーミングモードを追加) は、本体ライブラリの破壊的変更となるため **別 issue として `create-issue` 経由で起票** する。本 issue はその API 拡張が完了していることを前提に動かす。

### スコープ外

以下は本 issue では扱わない (別 issue で扱う、または将来検討):

- 上流接続のレスポンスボディストリーミング (現状の `stream_response_on_connection` で既に実装済み、本 issue は **リクエスト方向のみ**)
- HTTP/1.0 上流に対する chunked 送信 (RFC 9112 Section 7.1 は HTTP/1.1 必須、HTTP/1.0 上流で元が `Chunked` だった場合は 502 Bad Gateway を返す制約とする)
- WebSocket / HTTP/2 経路 (本サンプル未対応)
- バックプレッシャー制御の細かいチューニング (`BufWriter::flush` の await 経由で `tokio::io::AsyncWrite` が自然に伝搬する範囲のみ)

## 優先度根拠

High とする。ルート `README.md` の `http11_reverse_proxy` 機能説明では「ストリーミング転送」「chunked / content-length / close-delimited 対応」と謳われているが、リクエスト側は `Vec` に全ボディを貯めてから上流へ送信しており、実際にはストリーミングではない。大容量ボディやストリーミングクライアントに対して重大な問題となる。

## 現状

`examples/http11_reverse_proxy/src/main.rs:596-637` で `request_body = Vec::new()` に対し、`decoder.peek_body()` / `consume_body()` で全ボディを蓄積してから `upstream_request.body(request_body)` している。

その後 `examples/http11_reverse_proxy/src/main.rs:690-693` で接続プールから上流接続を取得し、`stream_response_on_connection` 内の `request.encode()` (`src/encoder.rs:822` 相当) でヘッダーとボディを一括エンコードして送信している。これにより、上流接続確立前に下流から全ボディを読み切る必要があり、メモリと TTFB の両方で不利になっている。

## 設計方針

1. 上流接続確立後、下流ボディを読みながら上流に即座に書き出すパイプライン処理に変更する。

2. `BodyKind` に応じた終端判定を正しく行う。リクエストメッセージでは `BodyKind::ContentLength` / `BodyKind::Chunked` / `BodyKind::None` の 3 経路のみが存在し、`BodyKind::CloseDelimited` は存在しない (RFC 9112 Section 6.3 item 7: request message body length is zero unless framed by Content-Length or chunked)。

3. リクエストのフレーミングは元の `BodyKind` を **そのまま維持** する。元が `ContentLength` なら `Content-Length` ヘッダーを保持したまま固定長で流す。元が `Chunked` なら `Transfer-Encoding: chunked` を維持して chunk 単位で流す。`BodyKind::None` ではボディを送信しない (元リクエストに `Content-Length: 0` がある場合はそのまま `Content-Length: 0` を維持する)。`Transfer-Encoding: chunked` への自動変換は **しない** (HTTP/1.0 上流互換性のため)。

4. `Request::encode()` (`src/encoder.rs:822`) の代わりに `Request::encode_headers()` (`src/encoder.rs:1078`、内部で `encode_request_headers` 関数 `src/encoder.rs:915` を呼び出す) を使い、ヘッダー送信後に固定長 write_all または `encode_chunk` でボディをストリーミング送信する。レスポンス側の `stream_response_on_connection` (`examples/http11_reverse_proxy/src/main.rs:940-1010`) と同じパターンを適用する。

5. `encode_request_headers` は `Content-Length` ヘッダー値と `Request::body_bytes()` の長さが一致しない場合に `EncodeError::ContentLengthMismatch` を返す (`src/encoder.rs:936-947`)。本 issue は **前提となる本体ライブラリ API 拡張** (上記「前提」節参照) で `encode_request_headers` にストリーミング用オプション (ボディ長検証をスキップするモード) が追加されていることを前提とし、本 issue 内ではそのオプション付きで呼び出す。**具体的なシグネチャは前提 issue で確定する** (本 issue では候補列挙しない、二重決定を避けるため)。

6. 上流への書き込みは `tokio::io::BufWriter` でバッファリングしつつ、chunk 境界や固定長の区切りで適宜 `flush()` する。`BufWriter::flush().await` の await ポイントが下流 read への自然なバックプレッシャーとして機能する (上流が遅れれば下流 read も停止する)。

7. `Expect: 100-continue` の処理:
   - 下流リクエストに `Expect: 100-continue` (RFC 9110 Section 10.1.1) が含まれる場合、**上流へのヘッダー送信成功 (= `encode_request_headers` の write_all 完了) 後** に下流に `HTTP/1.1 100 Continue\r\n\r\n` を送信してから下流ボディの読み取りを開始する (上流接続確立だけでなくヘッダー送信成功まで確認することで、上流書き込み失敗時に下流が無駄にボディ送信開始するのを防ぐ)。
   - 上流接続確立失敗または上流ヘッダー送信失敗時は下流に `100 Continue` を返さず、代わりに 502 Bad Gateway を返す。
   - 上流が早期に最終レスポンスを返した場合 (Expectation Failed 等) は、それを下流に転送して下流ボディ送信をキャンセルする。RFC 9110 Section 10.1.1 上は client は Continue 受信前にボディを送らない (SHOULD/MAY 条項あり、MUST ではない) ため、下流クライアントがレスポンス受信前に半送信ボディを送ってくる可能性がある。この場合 **下流ソケットを close する (drain せず接続を破棄する)**。半送信バイトを drain して破棄するループは DoS 経路 (Slowloris 系) となるため取らない。

8. HTTP/1.0 上流への送信制約 (RFC 9112 Section 7.1 は chunked を HTTP/1.1 必須と規定):
   - 元下流リクエストが `Chunked` で、上流接続が HTTP/1.0 として確立した場合は 502 Bad Gateway を返す (chunked → HTTP/1.0 変換は本 issue では実装しない、設計方針 3 の「自動変換しない」と整合)。
   - 元下流リクエストが `ContentLength` の場合は HTTP/1.0 上流でもそのまま転送可能。
   - 元下流リクエストが `None` の場合は HTTP/1.0 上流でもそのまま転送可能。

9. エラーハンドリングと rollback:
   - 下流ボディ読み取り中に下流切断・タイムアウトが発生した場合、上流接続を **プールに返却せずクローズ** する (RFC 9112 Section 6.3 item 6: 不完全なメッセージは持続接続にしない)。上流に途中まで送信したボディは破棄される。
   - 上流書き込み中に上流切断が発生した場合も、同じく上流接続を破棄する。下流には 502 Bad Gateway を返す (まだ下流に最終レスポンス送信前なら)。
   - ボディ転送中の各 await ポイント (`socket.read` / `upstream.write_all` / `BufWriter::flush`) は `tokio::time::timeout` でラップし、本 issue 専用定数 `const REQUEST_BODY_TRANSFER_TIMEOUT_SECS: u64 = 30;` を超えたら同様に rollback する。0111 のアイドル 60 秒 (`DOWNSTREAM_IDLE_TIMEOUT_SECS`) や上流プール側 idle 60 秒とは目的が異なるため (本値はボディ転送中の per-await 上限) 定数を共有しない。

10. `DecoderLimits::max_body_size` の取扱い:
    - 現状 `examples/http11_reverse_proxy/src/main.rs` で `50 * 1024 * 1024` (50 MiB) が設定されている (issue 1 周目で確認)。本 issue でストリーミング化した後も「下流から受信するボディの累積総量上限」として意味を保つ (DoS 対策)。
    - 100 MiB のような大容量を想定する場合は明示的に `DecoderLimits::max_body_size` を増やすが、本 issue の主目的は「Vec 蓄積を解消する」ことであり、上限値の調整自体は副次事項。50 MiB 維持で本 issue 完了とし、上限値の見直しは別途 issue 化する。

## 完了条件

- 本体ライブラリ側に encoder API 拡張 (`encode_request_headers` のストリーミングモード) を追加する別 issue が起票され、本 issue 着手時点でその issue がマージ済みであること。
- リクエストボディが `Vec` に全量貯められないこと。
- `BodyKind::ContentLength` / `BodyKind::Chunked` / `BodyKind::None` の各経路でストリーミング転送が機能すること。
- 大容量アップロード時のメモリ使用量が大幅に削減されること (例: 50 MiB ボディでもヒープ使用量が読み取りバッファ程度に抑えられる)。
- `DecoderLimits::max_body_size` は現状の 50 MiB を維持する (上限値の見直しは別 issue)。
- 下流クライアントが途中で切断した場合、上流への不完全なボディ送信が行われず、上流接続がプールに返却されないこと。
- 上流書き込みが途中で失敗した場合、上流接続がプールに返却されず、下流に 502 Bad Gateway が返ること (最終レスポンス未送信時)。
- 上流接続の取得に失敗した場合、下流クライアントに 502 Bad Gateway が返ること。
- 下流リクエストに `Expect: 100-continue` が含まれる場合、上流接続確立成功時に下流へ 100 Continue を送信してからボディ読み取りを開始すること。上流接続確立失敗時は 100 Continue を返さず 502 を返すこと。
- 元下流リクエストが `Chunked` で上流が HTTP/1.0 として確立した場合、502 Bad Gateway を返すこと。
- 各ボディ転送 await ポイント (`socket.read` / `upstream.write_all` / `BufWriter::flush`) は `tokio::time::timeout` (30 秒) でラップされ、超過時は接続破棄 + 502 経路で rollback すること。
- 0111 で新設される `examples/http11_reverse_proxy/tests/` ディレクトリに以下の検証を追加すること。モックやスタブは使用しない。
  - chunked リクエストの大容量ストリーミング転送
  - content-length リクエストの大容量ストリーミング転送
  - `Expect: 100-continue` 経路 (上流成功時 / 失敗時)
  - 上流 HTTP/1.0 + 下流 Chunked の 502 経路
  - 下流切断時の上流接続 non-reuse 確認
- テストで upstream として用いる実サーバーは Rust 内蔵の最小 echo サーバー (`examples/http11_reverse_proxy/tests/common/` 配下に新設) を起動する。nginx 等の外部依存は CI 移植性のため避ける (AGENTS.md「モックやスタブは利用しない」の趣旨は「実通信路を持つ実サーバーを使う」であり、自前 echo サーバーはモックではない)。echo サーバーは以下の機能要件を満たすこと:
  - HTTP/1.1 と HTTP/1.0 の両方を選択して起動可能 (HTTP/1.0 上流テスト用)
  - リクエストボディを受信して `Content-Length` または chunked で echo 返却 (大容量検証用)
  - `Expect: 100-continue` 受信時に「100 Continue を返す経路」と「Expectation Failed (417) を返す経路」の両モードを test harness から切り替え可能
  - 受信バイト累積サイズと TCP 切断タイミングを test harness から観測可能 (中断テスト用)
- `CHANGES.md` の `## develop` セクションの `### misc` 配下に `[FIX]` エントリを追加すること (本体ライブラリではなく `examples/` 配下の修正のため、`### misc` 配下)。例: `[FIX] examples/http11_reverse_proxy のリクエストボディ転送が一括バッファリングになっていた問題を修正する` (語尾は他の `[FIX]` エントリと合わせて「修正する」)。

## ファイル名・Branch 名の整合

現状: ファイル名 `0115-bug-reverse-proxy-request-body-buffering.md` / Branch `feature/fix-reverse-proxy-request-body-streaming`。「buffering」と「streaming」で命名が乖離している。本 issue 完了時に Branch 名を `feature/fix-reverse-proxy-request-body-buffering` に統一する (ファイル名側は履歴保持のため変更しない)。

## 解決方法

設計方針に沿って次の順序で実装する (詳細は設計方針節)。

1. 前提となる本体ライブラリ API 拡張 (`encode_request_headers` のストリーミングモード) を別 issue として `create-issue` 経由で起票し、本 issue 着手前にマージする。
2. `handle_client` のリクエストボディ読み出し部分を上流への書き込みと連動させる。`upstream_request` は `body()` を呼ばずヘッダーのみを構築し、`encode_request_headers` の **ストリーミングモード** で送信する。
3. `BodyKind::ContentLength` の場合:
   - 元の `Content-Length` 値を `upstream_request` に引き継ぐ (`examples/http11_reverse_proxy/src/main.rs:665` の除外を条件付きで保持)。
   - 1 で導入したストリーミングモードで `ContentLengthMismatch` を回避する。
   - `decoder.peek_body()` / `consume_body()` で得たバイト列を上流へ直接 `write_all` する。
   - 規定バイト数に達したら終端 + `flush()`。
4. `BodyKind::Chunked` の場合:
   - 元の `Transfer-Encoding: chunked` を引き継ぐ。
   - `decoder.peek_body()` で得たチャンクデータを `encode_chunk` でフレーミングして上流へ送信する。
   - `BodyProgress::Complete { trailers }` 時に終端チャンク (`0\r\n` + trailers + `\r\n`) を送信する。
   - 上流が HTTP/1.0 の場合は 502 Bad Gateway で rollback (設計方針 8)。
5. `BodyKind::None` の場合:
   - ボディを送信せず、ヘッダーのみを上流へ送信する。元リクエストに `Content-Length: 0` があればそのまま転送。
6. `Expect: 100-continue` 経路 (設計方針 7) を実装する。
7. 各 await ポイントを `tokio::time::timeout(Duration::from_secs(30), ...)` でラップし、超過時の rollback 経路を実装する (設計方針 9)。
8. 0111 で新設される `examples/http11_reverse_proxy/tests/` ディレクトリに `tests/common/` の Rust 製 echo サーバー (最小 HTTP/1.1 echo) を追加し、結合テストを書く。
9. Branch 名を `feature/fix-reverse-proxy-request-body-buffering` に統一する。
10. `CHANGES.md` の `## develop` セクションの `### misc` 配下に `[FIX] examples/http11_reverse_proxy のリクエストボディ転送が一括バッファリングになっていた問題を修正する` を追加する。

## 参考: RFC 文面

- RFC 9110 Section 8.6 Content-Length
  > a sender MUST NOT forward a message with a Content-Length header field value that is known to be incorrect.
- RFC 9110 Section 10.1.1 Expect
  > The "Expect" header field in a request indicates a certain set of behaviors (expectations) that need to be supported by the server in order to properly handle this request.
  > A client that will wait for a 100 (Continue) response before sending the request content MUST send an Expect header field containing a "100-continue" expectation.
- RFC 9112 Section 6.3 Message Body Length, item 7
  > If this is a request message and none of the above are true, then the message body length is zero (no message body is present).
- RFC 9112 Section 6.3 Message Body Length, item 6
  > If this is a request message and none of the above are true, then the message body length is zero ... ; a server MUST close the connection if it does not know the body length.
- RFC 9112 Section 7.1 Chunked Transfer Coding
  > The chunked transfer coding wraps the content in order to transfer it as a series of chunks ... The chunked transfer coding is complete when a chunk with a chunk-size of zero is received, possibly followed by a trailer section, and finally terminated by an empty line.
- RFC 9112 Section 6.1 Transfer-Encoding
  > A sender MUST NOT apply the chunked transfer coding more than once to a message body (i.e., chunking an already chunked message is not allowed).
  > A client MUST NOT send a request containing Transfer-Encoding unless it knows the server will handle HTTP/1.1 requests (or later); such knowledge might be in the form of specific user configuration or by remembering the version of a prior received response.
