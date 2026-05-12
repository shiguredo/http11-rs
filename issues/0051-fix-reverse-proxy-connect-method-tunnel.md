# 0051: reverse_proxy サンプルで CONNECT メソッドを 405 で拒否し接続クローズする

Created: 2026-05-12
Model: Opus 4.7

## 概要

`examples/http11_reverse_proxy/src/main.rs` (L343-345) は CONNECT メソッドの特別扱いが無く、通常の reverse 経路に流して `Request::new("CONNECT", req_head.uri())?` を構築する。upstream が 2xx を返すと `BodyKind::Tunnel` 判定になるが、`peek_body()` は Tunnel 状態で常に `None` を返すため `socket.read()` で永久ブロックし、ハンドラタスクと接続プールが滞留する。

本 issue は CONNECT を受信したら **405 Method Not Allowed を返して接続クローズ** する。reverse proxy としては固定 upstream への中継が責務であり、任意宛先へのトンネル化 (forward proxy 機能) はお手本サンプルのスコープ外とする。トンネル機能が必要な場合は別 example (`examples/http11_forward_proxy` 等) として別 issue で扱う。

## 根拠

### 現状コード

```rust
// examples/http11_reverse_proxy/src/main.rs:343-345
// アップストリームへプロキシリクエストを作成
let mut upstream_request = Request::new(req_head.method(), req_head.uri())?;
```

- L345 以降: CONNECT request を `--upstream` で指定された固定 upstream に転送するが、CONNECT 宛先 (request-target authority-form) と `--upstream` 指定は独立した値なので、本来クライアントが意図した宛先には届かない
- L478: `upstream.write_all(&request_bytes)` で `--upstream` 指定先に CONNECT を投げる (誤った宛先)
- L491: `decoder.set_request_method("CONNECT")` → upstream 2xx で `BodyKind::Tunnel`
- L624: `BodyKind::Tunnel` のループに入り、`peek_body() == None` で `socket.read()` 永久ブロック
- `tokio::io::copy_bidirectional` の呼び出しは grep 結果 0 件、`req_head.method() == "CONNECT"` の分岐も 0 件

### reverse proxy としての位置付け

reverse proxy は固定 upstream への中継が責務 (forward proxy と区別される)。CONNECT は任意宛先へのトンネルを要求するメソッドで、reverse proxy が受信した場合は以下のいずれかが妥当:

- (a) **405 Method Not Allowed で拒否** ← 本 issue で採用
- (b) CONNECT 宛先と `--upstream` の authority が一致した場合のみ受理 (実用シナリオ希薄)
- (c) forward proxy として任意宛先に飛ばす (reverse proxy の責務逸脱)

サンプルとしては (a) が最もシンプルで誤誘導がなく、AGENTS.md「サンプルは **お手本** なので性能と堅牢性を両立」と整合する。トンネル実装のお手本が必要なら別 example として独立に書く。

### RFC 引用

- RFC 9110 §9.3.6 (CONNECT): 「The CONNECT method requests that the recipient establish a tunnel to the destination origin server identified by the request-target」「Any 2xx (Successful) response indicates that the sender (and all inbound proxies) will switch to tunnel mode immediately after the response header section」
- RFC 9110 §15.5.6 (405 Method Not Allowed): 「The method received in the request-line is known by the origin server but not supported by the target resource」。reverse proxy が CONNECT をサポートしない旨を 405 で返すのは仕様準拠
- RFC 9110 §10.2.1 (Allow): 405 レスポンスでは `Allow` ヘッダーで許可メソッド一覧を返す MUST

### 影響

- 現状: CONNECT 受信で永久ハング、ハンドラタスクとプール接続が滞留 (DoS 経路)
- 修正後: CONNECT は即座に 405 + `Allow: GET, HEAD, POST, PUT, DELETE, OPTIONS, PATCH` を返して接続クローズ
- forward proxy 機能が必要なら別 example として実装するため、本サンプルの「reverse proxy」としての責務範囲が明確になる

## スコープ

- `handle_client` の `decode_headers` 完了直後に method 判定を入れ、CONNECT なら 405 Method Not Allowed + `Allow` ヘッダー + `Connection: close` を返して接続クローズする
- 含まない:
  - CONNECT トンネルの実装 (任意宛先への双方向リレー、別 example で扱う)
  - `--upstream` の authority と CONNECT 宛先が一致するケースの特別扱い
  - `tokio::io::copy_bidirectional` の導入

## 対応方針

### `handle_client` の CONNECT 分岐

`decode_headers` 完了直後で method を判定し、CONNECT (および将来的に対象外の method) を 405 で reject する:

```rust
if req_head.method().eq_ignore_ascii_case("CONNECT") {
    let response = Response::with_status(StatusCode::METHOD_NOT_ALLOWED)
        .header("Allow", "GET, HEAD, POST, PUT, DELETE, OPTIONS, PATCH")?
        .header("Connection", "close")?
        .header("Content-Length", "0")?;
    let bytes = response.try_encode()?;
    downstream.write_all(&bytes).await?;
    downstream.flush().await?;
    return Ok(false);  // can_reuse = false で keep-alive せず close
}
```

method 比較は RFC 9110 §9.1「method tokens are case-sensitive」に従い厳格な case-sensitive で `req_head.method() == "CONNECT"` でも可。`eq_ignore_ascii_case` は寛容寄りなのでサーバ側で寛容に倒すかの判断は実装者裁量。

### upstream 接続を発行しない

CONNECT 分岐に入った時点で upstream への TCP 接続や接続プール操作は **行わない**。直接 downstream に 405 を返して return する。

### 接続プール / Keep-Alive

`Connection: close` を返すため、本リクエスト処理後は downstream 接続を閉じる。`handle_client` の戻り値で `can_reuse = false` を返し、上位の `loop` が接続終了を判断できるようにする。

### テスト

`examples/http11_reverse_proxy/tests/` (新設) または `tests/helpers/mod.rs` 経由で integration test を追加:

```sh
# 本サンプルを起動
cargo run -p http11_reverse_proxy &
PROXY_PID=$!
sleep 1

# CONNECT を投げて 405 が返ることを確認
printf 'CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n' | nc -q 1 127.0.0.1 8080 | grep -q '405 Method Not Allowed'

kill $PROXY_PID
```

- 405 ステータスが返ること
- `Allow` ヘッダーに HTTP の標準メソッド (GET / HEAD / POST 等) が列挙されていること
- レスポンス送信後に接続が close されていること
- GET / POST 等の通常メソッドが従来通り upstream に転送されることを既存挙動として確認 (リグレッション防止)

### CHANGES.md

サンプルの機能不全修正は機能に直接影響するため `### misc` ではなく本体 `[FIX]` 配下 (0048 / 0049 / 0050 と方針統一):

```
- [FIX] `examples/http11_reverse_proxy` で CONNECT メソッドを 405 Method Not Allowed で拒否するように変更する
  - 旧実装は CONNECT を通常の reverse 経路に流して `--upstream` 指定先に転送し、upstream が 2xx を返した後 `BodyKind::Tunnel` 状態で `peek_body()` が永久に None を返すループでハンドラタスクがハングしていた
  - reverse proxy は固定 upstream への中継が責務であり、任意宛先へのトンネル化 (forward proxy 機能) はサンプルのスコープ外
  - CONNECT 受信時に 405 + `Allow` ヘッダー + `Connection: close` を返して接続クローズする (RFC 9110 §15.5.6 / §10.2.1 準拠)
  - トンネル機能のお手本が必要な場合は別 example として独立に追加する
  - @voluntas
```

### ブランチ

`feature/fix-reverse-proxy-connect-method-tunnel` (`feature/fix-` prefix、example 内部の修正のみで本体 API には影響なし、issue 番号を含まない)。

## 受け入れ基準

- `handle_client` の `decode_headers` 完了直後に CONNECT 判定が追加され、CONNECT 受信時は 405 + `Allow` + `Connection: close` + `Content-Length: 0` を返して接続クローズする
- CONNECT 受信時に upstream 接続が確立されない (TCP / TLS / プール取得を呼ばない)
- 405 レスポンス送信後にハンドラタスクがハングせず即終了する
- GET / POST / HEAD などの通常メソッドが従来通り upstream に転送されることがリグレッションテストで確認されている
- `examples/http11_reverse_proxy/tests/` に CONNECT 405 拒否の integration test が追加されている (test 基盤の新設を含む)
- `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` がすべて PASS
- CHANGES.md `## develop` に `[FIX]` エントリが追加されている

## 関連 issue

- 0050: 同じファイル対象、upstream URL の scheme/port。本 issue マージ後に rebase
- 0052: 同じファイル対象、close-delimited 経路の decoder バッファ取りこぼし

3 issue (0050 / 0051 / 0052) は同一ファイル (`examples/http11_reverse_proxy/src/main.rs`) の独立した症状で、0050 → 0051 → 0052 の順で着手する。

## 将来 issue (本 issue のスコープ外)

- `examples/http11_forward_proxy` 新設: CONNECT トンネル + GET 系の forward proxy 動作。`tokio::io::copy_bidirectional` を用いた双方向リレー、CONNECT 宛先のホワイトリスト管理、`RequestDecoder::take_remaining` 経由の先行バイト flush 等を含む。お手本として独立した実装が望ましい

## RFC 参照

- RFC 9110 §9.1 (method tokens は case-sensitive、`refs/rfc9110.txt`)
- RFC 9110 §9.3.6 (CONNECT メソッド、トンネル化)
- RFC 9110 §10.2.1 (Allow ヘッダー、405 で必須)
- RFC 9110 §15.5.6 (405 Method Not Allowed)
- RFC 9112 §3.2.3 (authority-form request-target、`refs/rfc9112.txt`)
