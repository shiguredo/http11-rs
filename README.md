# http11-rs

[![crates.io](https://img.shields.io/crates/v/shiguredo_http11.svg)](https://crates.io/crates/shiguredo_http11)
[![docs.rs](https://docs.rs/shiguredo_http11/badge.svg)](https://docs.rs/shiguredo_http11)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![GitHub Actions](https://github.com/shiguredo/http11-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/shiguredo/http11-rs/actions/workflows/ci.yml)
[![Discord](https://img.shields.io/badge/Discord-%235865F2.svg?logo=discord&logoColor=white)](https://discord.gg/shiguredo)

## About Shiguredo's open source software

We will not respond to PRs or issues that have not been discussed on Discord. Also, Discord is only available in Japanese.

Please read <https://github.com/shiguredo/oss> before use.

## 時雨堂のオープンソースソフトウェアについて

利用前に <https://github.com/shiguredo/oss> をお読みください。

## 概要

Rust で実装された依存 0 かつ Sans I/O な HTTP/1.1 スタイル テキストプロトコルライブラリです。HTTP/1.1 の他、RTSP/1.0, RTSP/2.0 等のプロトコルでも利用できます。

## 特徴

- Sans I/O
  - <https://sans-io.readthedocs.io/index.html>
- `no_std` 対応
  - `alloc` クレートは必要
- 依存ライブラリ 0
- 圧縮/展開トレイト (`Compressor` / `Decompressor`) の提供
  - ライブラリ本体は圧縮実装を含まず、利用者が任意の実装を組み込める

## 使い方

### クライアント (リクエスト送信、レスポンス受信)

```rust
use shiguredo_http11::{Request, ResponseDecoder};

// リクエストを作成してエンコード
let request = Request::new("GET", "/")
    .header("Host", "example.com")
    .header("Connection", "close");
let bytes = request.encode();
// bytes を送信...

// レスポンスをデコード
let mut decoder = ResponseDecoder::new();
// 受信データを feed...
// decoder.feed(&received_data)?;
// if let Some(response) = decoder.decode()? { ... }
```

### サーバー (リクエスト受信、レスポンス送信)

```rust
use shiguredo_http11::{RequestDecoder, Response};

// リクエストをデコード
let mut decoder = RequestDecoder::new();
// 受信データを feed...
// decoder.feed(&received_data)?;
// if let Some(request) = decoder.decode()? { ... }

// レスポンスを作成してエンコード
let response = Response::new(200, "OK")
    .header("Content-Type", "text/plain")
    .body(b"Hello, World!".to_vec());
let bytes = response.encode();
// bytes を送信...
```

### 圧縮/展開 (Content-Encoding)

ライブラリ本体は圧縮/展開の実装は含みません。
代わりに `Compressor` / `Decompressor` トレイトを提供し、利用者が任意の実装を組み込めます。

```rust
use shiguredo_http11::{ResponseDecoder, ResponseEncoder};
use shiguredo_http11::compression::{Compressor, Decompressor, NoCompression};

// 展開器を組み込んだデコーダー
let decoder = ResponseDecoder::with_decompressor(MyGzipDecompressor::new());

// 圧縮器を組み込んだエンコーダー
let encoder = ResponseEncoder::with_compressor(MyGzipCompressor::new());

// 従来通りの使い方 (圧縮なし)
let decoder = ResponseDecoder::new(); // NoCompression がデフォルト
```

サンプル (`examples/`) では `flate2`, `brotli`, `zstd` クレートを使った実装例を提供しています。

### HEAD リクエストの処理

HEAD リクエストへのレスポンスは、RFC 9110 Section 9.3.2 に基づき GET と同じヘッダーを返しますがボディは送信しません。

```rust
use shiguredo_http11::{Request, Response, ResponseDecoder};

// サーバー側: HEAD リクエストへのレスポンス
// RFC 9110 Section 9.3.2: GET と同じヘッダーを返すがボディは送信しない
let is_head = request.method.eq_ignore_ascii_case("HEAD");

let body = b"Hello, World!";
let mut response = Response::new(200, "OK")
    .header("Content-Type", "text/plain")
    .header("Content-Length", &body.len().to_string())
    .omit_body(is_head);

if !is_head {
    response = response.body(body.to_vec());
}
let bytes = response.encode();

// クライアント側: HEAD レスポンスの受信
let request = Request::new("HEAD", "/")
    .header("Host", "example.com");
let bytes = request.encode();
// bytes を送信...

let mut decoder = ResponseDecoder::new();
decoder.set_expect_no_body(true); // HEAD レスポンスではボディなし
// decoder.feed(&received_data)?;
// if let Some(response) = decoder.decode()? { ... }
```

### ストリーミングエンコード

ヘッダーのみをエンコードし、後からボディをチャンクで送信できます。

```rust
use shiguredo_http11::{Response, encode_chunk};

let response = Response::new(200, "OK")
    .header("Transfer-Encoding", "chunked");
let headers = response.encode_headers();
// headers を送信...

// チャンクを送信
let chunk1 = encode_chunk(b"Hello, ");
let chunk2 = encode_chunk(b"World!");
let last = encode_chunk(b""); // 終端チャンク
```

メソッド形式 (`request.encode()` / `response.encode_headers()`) と等価な関数形式 API も提供しています。

- `encode_request` / `encode_response` - リクエスト/レスポンス全体をエンコード
- `encode_request_headers` / `encode_response_headers` - ヘッダーのみをエンコード
- `encode_chunk` - 単一チャンクをエンコード (終端は `b""`)
- `encode_chunks` - 複数チャンクをまとめてエンコード
- `RequestEncoder` / `ResponseEncoder` - 圧縮器 (`Compressor`) を組み込んだエンコーダー

### ストリーミングデコード

大きなボディを扱う場合や、ボディを受信しながら処理したい場合はストリーミング API を使用します。

- `decode_headers()` - ヘッダーをデコードして `(RequestHead/ResponseHead, BodyKind)` を返す
- `peek_body()` - 利用可能なボディデータをゼロコピーで取得
- `consume_body(len)` - ボディデータを消費して `BodyProgress` を返す
- `progress()` - 状態機械を進める (Chunked のチャンクサイズ行パース等)
- `mark_eof()` - 接続終了を通知 (close-delimited ボディ用、ResponseDecoder のみ)

`BodyKind` はボディの種類を表します:

- `ContentLength(u64)` - Content-Length による固定長
- `Chunked` - Transfer-Encoding: chunked
- `CloseDelimited` - 接続終了までがボディ (レスポンスのみ、RFC 9112)
- `Tunnel` - CONNECT 2xx レスポンス後のトンネルモード (Transfer-Encoding/Content-Length は無視)
- `None` - ボディなし

`BodyProgress` はデコードの進捗を表します:

- `Continue` - 継続中
- `Complete { trailers }` - 完了 (トレーラーヘッダーがある場合は含む)

## HTTP/1.1

このライブラリが対応している HTTP/1.1 の仕組みです。

### Transfer-Encoding

- chunked 転送エンコーディングのデコード/エンコード
- チャンクサイズの 16 進数パース
- トレーラーヘッダーの処理
- chunk extension の受信処理 (RFC 9112 が specialized service 向けと明記、一般的に使われていない。受信時は内容を破棄)
- ストリーミング送信用のチャンクエンコード API

### Content-Length

- Content-Length ヘッダーのパース
- エンコード時の Content-Length 自動計算
- ボディサイズ制限によるチェック

### Connection

- Connection ヘッダー (keep-alive, close) の処理
- HTTP/1.1 デフォルトでの keep-alive 動作
- `is_keep_alive()` によるキープアライブ判定

### ボディ処理

- Transfer-Encoding: chunked が最優先
- Content-Length による固定長ボディ
- ステータスコード 1xx/204/304 はボディなし
- 205 Reset Content はエンコード時にボディ禁止 (RFC 9110 Section 15.3.6)
  - Transfer-Encoding 禁止、Content-Length は 0 のみ許可
- HEAD リクエストへのレスポンスはボディなし
- CONNECT 2xx レスポンスはトンネルモードに移行

### ヘッダー

- 大文字小文字を区別しないヘッダー名の比較
- 同一名ヘッダーの複数値対応
- ヘッダー数/行長の制限

### キャッシュ (RFC 9111)

- Cache-Control ヘッダー
  - max-age, s-maxage, max-stale, min-fresh
  - stale-while-revalidate, stale-if-error
  - no-cache, no-store, no-transform
  - only-if-cached, must-revalidate, proxy-revalidate
  - must-understand, public, private, immutable
- Age ヘッダー
- Expires ヘッダー

### 条件付きリクエスト (RFC 9110)

- If-Match / If-None-Match ヘッダー (ETag 比較)
- If-Modified-Since / If-Unmodified-Since ヘッダー
- If-Range ヘッダー (ETag または日時)

### Range リクエスト (RFC 9110)

- Range ヘッダーのパース (bytes=0-499, 500-, -500)
  - RangeSpec (Range, FromStart, Suffix)
  - 実際のバイト範囲計算 (to_bounds)
- Content-Range ヘッダーの生成
  - 満たせない範囲 (unsatisfied) の表現
- Accept-Ranges ヘッダー

### 認証 (RFC 7617, RFC 7616, RFC 6750)

- Basic / Digest / Bearer 認証のエンコード/デコード
- Authorization / WWW-Authenticate ヘッダー
- Proxy-Authorization / Proxy-Authenticate ヘッダー

### URI (RFC 3986)

- URI のパース (scheme, host, port, path, query, fragment)
- パーセントエンコーディング/デコーディング
  - 汎用 (percent_encode)
  - パス用 (percent_encode_path)
  - クエリ用 (percent_encode_query)
  - デコード (percent_decode, percent_decode_bytes)
- 相対 URI の解決
- URI の正規化 (normalize)
- origin-form 生成 (HTTP request-target 用)

### その他のヘッダー

- Content-Type (メディアタイプ、charset、boundary)
- Content-Encoding (gzip, deflate, compress, identity, 拡張エンコーディング対応)
- Content-Disposition (inline/attachment、filename、filename*)
- Content-Language
- Content-Location
- Date (HTTP-date 形式: IMF-fixdate (推奨)、RFC 850 / asctime (obs-date、廃止、受信のみ対応))
- ETag (Strong/Weak)
- Cookie / Set-Cookie
- Host ヘッダーのパース/検証 (IPv4, IPv6 リテラル, IPv-future 対応)
- Multipart (multipart/form-data)
- Trailer ヘッダー (RFC 9112 Section 7.1.2 禁止フィールド検証、一般的に使われていない)
- Expect ヘッダー
- Upgrade ヘッダー
- Content-Digest / Repr-Digest / Want-Content-Digest / Want-Repr-Digest (RFC 9530)

### コンテントネゴシエーション

- Accept (media-type, q 値)
- Accept-Charset (deprecated: RFC 9110 Section 12.5.2)
- Accept-Encoding
- Accept-Language
- Vary

### セキュリティ

- Response Splitting 対策: ヘッダー行の CR/LF と obs-fold を拒否
- Request Smuggling 対策: Transfer-Encoding と Content-Length の同時指定拒否
- Request Smuggling 対策: Content-Length の不一致検出

### 制限 (DoS 対策)

デフォルト値:

- 最大バッファサイズ: 64KB
- 最大ヘッダー数: 100
- 最大ヘッダー行長: 8KB
- 最大ボディサイズ: 10MB
- 最大チャンクサイズ行長: 64 bytes

`DecoderLimits` で各制限値をカスタマイズ可能です。

### 既知の制限事項

- obs-text (0x80-0xFF) の非 UTF-8 バイト列はヘッダー値として拒否されます。RFC 9110 では obs-text を含むフィールド値は構文上有効ですが、本ライブラリはヘッダー値を Rust の `String` として扱うため、非 UTF-8 バイト列を受け付けません。

## サンプル

サンプルは [Tokio](https://github.com/tokio-rs/tokio) と [Rustls](https://github.com/rustls/rustls) を利用しています。引数のライブラリには [noargs](https://github.com/sile/noargs) を利用しています。

### http11_client

HTTP/HTTPS クライアントの例です。

```bash
cargo run -p http11_client -- https://example.com/
cargo run -p http11_client -- http://httpbin.org/get
```

**機能:**

- HTTP/HTTPS リクエスト送信
- レスポンス受信とボディ表示
- rustls-platform-verifier による TLS 検証
- 圧縮レスポンスの展開 (gzip, br, zstd)

### http11_server

HTTP/HTTPS サーバーの例です。

```bash
cargo run -p http11_server -- --port 8080
cargo run -p http11_server -- --port 8443 --tls --cert cert.pem --key key.pem
```

**オプション:**

- `-p, --port <PORT>`: リッスンポート (デフォルト: 8080)
- `--tls`: HTTPS 有効化 (ポートデフォルト: 8443)
- `--cert <PATH>`: 証明書ファイル (PEM 形式)
- `--key <PATH>`: 秘密鍵ファイル (PEM 形式)

**機能:**

- HEAD リクエスト対応 (RFC 9110 Section 9.3.2)
- Keep-Alive 対応 (タイムアウト 60 秒、最大リクエスト数 1000)
- Accept-Encoding に基づく圧縮 (gzip, br, zstd)
- エンドポイント: `/` (HTML), `/info` (JSON), `/echo` (リクエスト詳細)

### http11_reverse_proxy

HTTP/HTTPS リバースプロキシの例です。

```bash
cargo run -p http11_reverse_proxy -- --port 8888 --upstream https://example.com
curl http://localhost:8888/
```

**オプション:**

- `-p, --port <PORT>`: リッスンポート (デフォルト: 8888)
- `-u, --upstream <URL>`: アップストリーム URL (デフォルト: <https://example.com>)
- `--debug`: デバッグログ有効化

**機能:**

- ストリーミング転送 (chunked, content-length, close-delimited 対応)
- 接続プール (ホストあたり最大 10 接続)
- hop-by-hop ヘッダーの処理
- HEAD リクエスト対応

### http11_server_io_uring

io_uring + kTLS を使った HTTPS サーバーの例です。Linux 専用です。

```bash
cargo run -p http11_server_io_uring -- --port 8443 --cert cert.pem --key key.pem
```

**前提条件:**

- Linux カーネル 6.7 以上
- CONFIG_TLS=y または CONFIG_TLS=m

**オプション:**

- `-p, --port <PORT>`: リッスンポート (デフォルト: 8443)
- `--cert <PATH>`: 証明書ファイル (PEM 形式)
- `--key <PATH>`: 秘密鍵ファイル (PEM 形式)

**機能:**

- io_uring SQPOLL モード
- kTLS (Kernel TLS) によるカーネルレベル暗号化
- HEAD リクエスト対応 (RFC 9110 Section 9.3.2)
- Keep-Alive 対応 (最大リクエスト数 1000)
- Accept-Encoding に基づく圧縮 (gzip, br, zstd)

## Agent Skills

[Agent Skills](https://agentskills.io/) 形式のスキルを同梱しています。`gh skill install` コマンドで対応する AI エージェント (Claude Code, Cursor, GitHub Copilot, Gemini CLI 等) にインストールでき、エージェントがこのライブラリの API や RFC 準拠仕様を理解した上で支援できるようになります。

```bash
gh skill install shiguredo/http11-rs shiguredo-http11
```

スキルの内容は [`skills/shiguredo-http11/SKILL.md`](skills/shiguredo-http11/SKILL.md) を参照してください。

## 規格書

このライブラリが準拠している RFC 一覧です。

- RFC 3986 - Uniform Resource Identifier (URI): Generic Syntax
  - <https://datatracker.ietf.org/doc/html/rfc3986>
- RFC 6265 - HTTP State Management Mechanism
  - <https://datatracker.ietf.org/doc/html/rfc6265>
- RFC 6266 - Use of the Content-Disposition Header Field in the Hypertext Transfer Protocol (HTTP)
  - <https://datatracker.ietf.org/doc/html/rfc6266>
- RFC 6750 - The OAuth 2.0 Authorization Framework: Bearer Token Usage
  - <https://datatracker.ietf.org/doc/html/rfc6750>
- RFC 7578 - Returning Values from Forms: multipart/form-data
  - <https://datatracker.ietf.org/doc/html/rfc7578>
- RFC 7616 - HTTP Digest Access Authentication
  - <https://datatracker.ietf.org/doc/html/rfc7616>
- RFC 7617 - The 'Basic' HTTP Authentication Scheme
  - <https://datatracker.ietf.org/doc/html/rfc7617>
- RFC 8187 - Indicating Character Encoding and Language for HTTP Header Field Parameters
  - <https://datatracker.ietf.org/doc/html/rfc8187>
- RFC 9110 - HTTP Semantics
  - <https://datatracker.ietf.org/doc/html/rfc9110>
- RFC 9111 - HTTP Caching
  - <https://datatracker.ietf.org/doc/html/rfc9111>
- RFC 9112 - HTTP/1.1
  - <https://datatracker.ietf.org/doc/html/rfc9112>
- RFC 9530 - Digest Fields
  - <https://datatracker.ietf.org/doc/html/rfc9530>

## ライセンス

Apache License 2.0

```text
Copyright 2026-2026, Shiguredo Inc.

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```
