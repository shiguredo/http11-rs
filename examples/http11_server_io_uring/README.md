# http11_server_io_uring

io_uring + kTLS を使った HTTP/1.1 サーバーのサンプル (Linux 専用)

## 概要

このサンプルは `shiguredo_http11` ライブラリを使用して、io_uring と kTLS を活用した高性能な HTTPS サーバーを実装しています。

- io_uring の SQPOLL モードによる高効率 I/O
- kTLS (Kernel TLS) によるカーネル空間での TLS 処理
- HTTP Keep-Alive 対応
- Accept-Encoding に基づく圧縮対応 (gzip, br, zstd)

## システム要件

- Linux カーネル 6.7 以上
  - io_uring setsockopt サポートが必要
- `CONFIG_TLS=y` または `CONFIG_TLS=m`
- tls カーネルモジュールがロード済み

```bash
# カーネルモジュールのロード
sudo modprobe tls
```

## 使い方

### 自己署名証明書の作成

```bash
openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -days 365 -nodes \
  -subj "/CN=localhost"
```

### サーバーの起動

```bash
cargo run -p http11_server_io_uring -- --cert cert.pem --key key.pem
```

### コマンドラインオプション

| オプション | 短縮形 | 説明 | デフォルト |
|-----------|--------|------|-----------|
| `--cert` | - | 証明書ファイル (PEM) | 必須 |
| `--key` | - | 秘密鍵ファイル (PEM) | 必須 |
| `--port` | `-p` | リッスンポート | 8443 |
| `--version` | `-V` | バージョン表示 | - |
| `--help` | `-h` | ヘルプ表示 | - |

## エンドポイント

| パス | 説明 |
|------|------|
| `/` | ウェルカムページ (HTML) |
| `/info` | サーバー情報 (JSON) |
| `/echo` | リクエスト詳細のエコー |

## 圧縮対応

クライアントの `Accept-Encoding` ヘッダーに基づいて自動的に圧縮を適用します。

優先順位: zstd > br > gzip

gzip / br / zstd の 3 形式すべてが常に有効です。

## 動作確認

```bash
# curl でアクセス (自己署名証明書のため -k オプション)
curl -k https://localhost:8443/
curl -k https://localhost:8443/info
curl -k https://localhost:8443/echo

# 圧縮を有効にしてアクセス
curl -k -H "Accept-Encoding: gzip" https://localhost:8443/ --compressed
curl -k -H "Accept-Encoding: br" https://localhost:8443/ --compressed
curl -k -H "Accept-Encoding: zstd" https://localhost:8443/ --compressed
```

## 技術詳細

### io_uring 操作

- `Accept`: 新規接続の受け入れ
- `Read`: データ読み取り
- `Write`: データ書き込み
- `Close`: 接続クローズ
- `SetSockOpt`: kTLS 有効化

### kTLS 有効化シーケンス

1. TLS ハンドシェイク完了 (rustls)
2. TLS セッションキーの抽出
3. `TCP_ULP` を "tls" に設定
4. `TLS_TX` (送信方向) の暗号化情報を設定
5. `TLS_RX` (受信方向) の暗号化情報を設定

kTLS 有効化後は、カーネルが TLS レコードの暗号化/復号化を処理するため、アプリケーションは平文でデータを読み書きできます。

### 定数

| 定数 | 値 | 説明 |
|------|-----|------|
| `DEFAULT_KEEP_ALIVE_TIMEOUT` | 60秒 | Keep-Alive タイムアウト |
| `DEFAULT_MAX_REQUESTS` | 1000 | 1 接続あたりの最大リクエスト数 |
| `READ_BUF_SIZE` | 8KB | 読み取りバッファサイズ |
| `WRITE_BUF_SIZE` | 64KB | 書き込みバッファサイズ |
| `RING_ENTRIES` | 256 | io_uring のエントリ数 |

## 依存クレート

| クレート | バージョン | 説明 |
|---------|-----------|------|
| `shiguredo_http11` | - | HTTP/1.1 パーサー |
| `io-uring` | 0.7+ | io_uring バインディング |
| `rustls` | 0.23 | TLS 実装 |
| `ktls` | 6.0.2+ | kTLS サポート |
| `noargs` | 0.4 | コマンドライン引数パーサー |
| `slab` | 0.4 | 接続管理 |
| `flate2` | 1 | gzip 圧縮 (オプション) |
| `brotli` | 8 | Brotli 圧縮 (オプション) |
| `zstd` | 0.13 | Zstandard 圧縮 (オプション) |

## 準拠仕様

- RFC 9110 (HTTP Semantics)
- RFC 9112 (HTTP/1.1)

## 手動テスト: HTTP/1.1 パイプライニング (issue 0048)

`curl` はデフォルトでパイプライニングを行わないため、`printf` + `nc` で 1 TCP segment に
複数リクエストを詰めて送る:

```sh
# サーバ起動 (TLS 無効では動かないため、TLS で接続して 1 segment に詰める)
# 本サンプルは kTLS 必須のため、Linux 環境で実行する
printf 'GET /a HTTP/1.1\r\nHost: localhost\r\n\r\nGET /b HTTP/1.1\r\nHost: localhost\r\n\r\n' \
  | openssl s_client -connect 127.0.0.1:8443 -quiet -ign_eof
```

期待: `/a` のレスポンスに続いて `/b` のレスポンスが順序通りに返ること。

issue 0048 の修正前は 1 件目のみ返り、2 件目以降は drop されていた。
