# 0051: reverse_proxy サンプルで CONNECT メソッドのトンネル化を実装する

Created: 2026-05-12
Model: Opus 4.7

## 概要

`examples/http11_reverse_proxy/src/main.rs` には CONNECT メソッドの特別扱いが存在しない。`tokio::io::copy_bidirectional` の利用も 0 件。クライアントから `CONNECT downstream.example:443 HTTP/1.1` を受けると、通常の reverse proxy 経路に流して `Request::new("CONNECT", req_head.uri())?` を作り、`Host: upstream_host` を上書きして上流へ転送する。

```rust
// examples/http11_reverse_proxy/src/main.rs:343-345
// アップストリームへプロキシリクエストを作成
let mut upstream_request = Request::new(req_head.method(), req_head.uri())?;
```

upstream が 2xx を返すと `BodyKind::Tunnel` 判定になるが、トンネル両方向リレー実装がないため `peek_body() == None` のループで永久ハングする。

## 根拠

### 処理フロー (実コード読解)

1. L271-291: `RequestDecoder` でクライアントからリクエストヘッダーを読む。`CONNECT example.com:443 HTTP/1.1` が `req_head` に入る
2. L301-342: `BodyKind::None` 経路 (CONNECT リクエストはボディなし)
3. L345: `Request::new(req_head.method(), req_head.uri())?` で **CONNECT + authority-form URI** をそのまま upstream に転送
4. L377-379: `Host: upstream_host` (CLI で指定した固定 upstream) と `Connection: keep-alive` を付与
5. L478: `upstream.write_all(&request_bytes)` で `--upstream` 指定先に転送 (**クライアントが CONNECT したい本来の宛先には届かない**)
6. L491: `decoder.set_request_method("CONNECT")` で 2xx 後の Tunnel 判定が走る
7. L624: `BodyKind::Tunnel` のループに入る
8. `peek_body` は Tunnel 状態で常に `None` を返す
9. L684 で upstream から read → 上流は CONNECT 後トンネルとして「クライアントが送るべき任意のバイト列」を待っている状態のため何も送ってこない
10. **永久ブロック**、ハンドラタスクとプール接続が枯渇

### RFC

- RFC 9110 §9.3.6: CONNECT 後 2xx は immediately after the header section にトンネルとなる
- RFC 9112 §9.3.3: authority-form の request-target で受信
- 双方向リレーはプロキシ実装の責務

### AGENTS.md との衝突

- 「サンプルは **お手本** なので性能と堅牢性を両立させること」
- HTTP/HTTPS proxy で CONNECT は基本動詞。未対応は致命的な機能不全

## 影響範囲

- HTTPS over HTTP proxy として動作しない (CONNECT 非対応)
- CONNECT を投げたクライアントが永久ハング、サーバ側もハンドラタスクとプール接続が滞留
- お手本としての要件不適合

## 対応方針

### `req_head.method() == "CONNECT"` の早期分岐

- 受信した CONNECT request の authority (request-target) をパースし、本来の宛先ホスト:ポートを取得する
- 上流 (CLI 指定 `--upstream` ではなく、CONNECT の宛先) に対して TCP 接続を張る
- TLS であれば本実装の責務は CONNECT-over-TLS まで (TLS 終端しない)
- 200 OK を downstream に返した後、`tokio::io::copy_bidirectional(downstream, upstream)` で双方向リレーする
- プールから取得した接続は CONNECT トンネル後は **必ず破棄** (再利用不可)

### 既存 reverse 経路との分離

- handler 関数の冒頭で method 判定して別関数に飛ばす
- error response (4xx / 5xx) のフォーマットを CONNECT 失敗時に正しく返す経路を設計する

### テスト

- curl `-x http://127.0.0.1:PORT https://example.com` で CONNECT 経由 HTTPS が動くことを確認する integration test
- CONNECT 4xx / 5xx で接続クローズを正しく返すことを確認する

### CHANGES.md

`## develop` の `### misc` に `[FIX]` として追加する。

### 関連 issue

- 0050 (upstream URL scheme/port) と並んで reverse_proxy の機能不全 3 連
- 0052 (close-delimited 取りこぼし) と並ぶ
