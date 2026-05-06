---
name: shiguredo-http11
description: 時雨堂の Sans I/O HTTP/1.1 ライブラリ shiguredo_http11 の機能・API リファレンス。リクエスト/レスポンスのエンコード・デコード、ストリーミング処理、ヘッダーパース、圧縮トレイト、RFC 準拠に関する質問時に使用。
---

# shiguredo_http11

Sans I/O 設計に基づく HTTP/1.1 パーサー/シリアライザーライブラリ。

## 特徴

- **依存なし**: 外部依存ゼロ (`core` / `alloc` のみ)
- **`no_std` 完全対応**: `std` 非依存 (`alloc` クレートは必要)
- **Sans I/O**: I/O を完全に分離した設計 (Tokio, async-std, 同期 I/O など任意の環境で使用可能)
- **柔軟性**: HTTP/1.1 のほか RTSP/1.0, RTSP/2.0 等のテキストプロトコルにも応用可能
- **ストリーミング対応**: 大容量ボディをメモリ効率的に処理
- **圧縮トレイト**: `Compressor` / `Decompressor` で任意の実装を組み込み可能 (本体は実装を含まない)
- **RFC 準拠**: RFC 9110, 9111, 9112 等の最新仕様に対応
- **セキュリティ**: Request Smuggling, Response Splitting 対策

## バージョン情報

- crate 名: `shiguredo_http11`
- バージョン: 2026.2.0
- Rust Edition: 2024
- 最小 Rust バージョン: 1.88
- ライセンス: Apache-2.0

## コア API

### エンコード用 (送信側)

| 型 | 説明 | 主要メソッド |
|----|------|-------------|
| `Request` | HTTP リクエスト | `new()`, `with_version()`, `header()`, `body()`, `encode()`, `try_encode()`, `encode_headers()`, `try_encode_headers()`, `is_keep_alive()`, `is_chunked()` |
| `Response` | HTTP レスポンス | `new()`, `with_version()`, `header()`, `body()`, `omit_body()`, `encode()`, `try_encode()`, `encode_headers()`, `try_encode_headers()`, `is_success()`, `is_redirect()`, `is_client_error()`, `is_server_error()`, `is_keep_alive()` |
| `RequestEncoder<C>` | 圧縮対応リクエストエンコーダー | `with_compressor()` |
| `ResponseEncoder<C>` | 圧縮対応レスポンスエンコーダー | `with_compressor()` |

`encode()` と `encode_headers()` は RFC 違反時にパニックする。エラーハンドリングが必要なら `try_encode()` / `try_encode_headers()` を使う。

### デコード用 (受信側)

| 型 | 説明 | 主要メソッド |
|----|------|-------------|
| `RequestDecoder<D>` | リクエストデコーダー | `new()`, `with_limits()`, `with_decompressor()`, `with_decompressor_and_limits()`, `feed()`, `feed_unchecked()`, `mut_buf()`, `advance_buf()`, `available_buf()`, `decode()`, `decode_headers()`, `peek_body()`, `consume_body()`, `progress()`, `remaining()`, `limits()`, `reset()` |
| `ResponseDecoder<D>` | レスポンスデコーダー | 同上 + `mark_eof()`, `is_close_delimited()`, `is_tunnel()`, `take_remaining()`, `set_expect_no_body()`, `set_request_method()` |
| `RequestHead` | デコード済みリクエストヘッダー | `method`, `uri`, `version`, `headers` |
| `ResponseHead` | デコード済みレスポンスヘッダー | `version`, `status_code`, `reason_phrase`, `headers` (+ `is_success()`, `is_redirect()`, `is_client_error()`, `is_server_error()`, `is_informational()`) |
| `HttpHead` | ヘッダー操作トレイト (`Request` / `Response` / `RequestHead` / `ResponseHead` が実装) | `version()`, `headers()`, `get_header()`, `is_keep_alive()`, `is_chunked()` |
| `request_target::RequestTargetForm` | request-target 形式 (encoder/decoder で共通) | `Origin`, `Absolute`, `Authority`, `Asterisk` |

### HttpHead トレイト

`Request` / `Response` / `RequestHead` / `ResponseHead` が実装する共通トレイト。エンコーダー側とデコーダー側でヘッダー操作の一貫性を保証する。

| メソッド | 説明 |
|----------|------|
| `version()` | HTTP バージョンを取得 |
| `headers()` | ヘッダーリストを取得 |
| `get_header(name)` | ヘッダーを取得 (大文字小文字を区別しない) |
| `get_headers(name)` | 同名ヘッダーをすべて取得 |
| `has_header(name)` | ヘッダーの存在確認 |
| `connection()` | Connection ヘッダーを取得 |
| `is_keep_alive()` | キープアライブ接続か判定 |
| `content_length()` | Content-Length を取得 (`Option<u64>`) |
| `is_chunked()` | Transfer-Encoding の最後が chunked か判定 |

### ボディ処理

| 型 | 説明 |
|----|------|
| `BodyKind::ContentLength(u64)` | Content-Length による固定長 |
| `BodyKind::Chunked` | Transfer-Encoding: chunked |
| `BodyKind::CloseDelimited` | 接続終了まで (レスポンスのみ) |
| `BodyKind::Tunnel` | CONNECT 2xx レスポンス後のトンネルモード (Transfer-Encoding/Content-Length は無視) |
| `BodyKind::None` | ボディなし |
| `BodyProgress::Advanced` | 状態機械が前進し、続けて呼び出すことで処理を進められる |
| `BodyProgress::NeedData` | 追加データが必要。呼び出し側はループを抜けて I/O に戻る |
| `BodyProgress::Complete { trailers }` | 完了 (トレーラーヘッダー含む) |

### エンコーダー関数

| 関数 | 戻り値 | 説明 |
|------|--------|------|
| `encode_request(&Request)` | `Result<Vec<u8>, EncodeError>` | リクエスト全体をエンコード |
| `encode_response(&Response)` | `Result<Vec<u8>, EncodeError>` | レスポンス全体をエンコード |
| `encode_request_headers(&Request)` | `Result<Vec<u8>, EncodeError>` | ヘッダーのみエンコード |
| `encode_response_headers(&Response)` | `Result<Vec<u8>, EncodeError>` | ヘッダーのみエンコード |
| `encode_chunk(&[u8])` | `Vec<u8>` | 単一チャンクをエンコード (空入力は終端チャンク) |
| `encode_chunks(&[&[u8]])` | `Vec<u8>` | 複数チャンクをエンコード (終端含む) |

## 圧縮トレイト

| 型/トレイト | 説明 |
|------------|------|
| `Compressor` | 圧縮トレイト: `compress()`, `finish()`, `reset()` |
| `Decompressor` | 展開トレイト: `decompress()`, `reset()` |
| `NoCompression` | デフォルト実装 (圧縮なし) |
| `CompressionStatus` | `Continue { consumed, produced }`, `Complete { consumed, produced }`, `OutputFull { consumed, produced }` |
| `CompressionError` | `BufferTooSmall { required, available }`, `InvalidData(String)`, `Internal(String)`, `UnexpectedEof`, `AlreadyFinished` |

`CompressionStatus` には `consumed()`, `produced()`, `is_complete()`, `is_output_full()` ヘルパーがある。

ライブラリ本体は圧縮実装を含まないため、利用者が `flate2`, `brotli`, `zstd` などを使って実装する。

## ヘッダーパースモジュール

| モジュール | 主要型 | RFC |
|-----------|--------|-----|
| `accept` | `Accept`, `AcceptCharset`, `AcceptEncoding`, `AcceptLanguage`, `QValue` | RFC 9110 |
| `auth` | `BasicAuth`, `DigestAuth`, `DigestChallenge`, `BearerToken`, `BearerChallenge`, `WwwAuthenticate`, `Authorization`, `AuthChallenge`, `ProxyAuthorization`, `ProxyAuthenticate`, `AuthError` | RFC 7617, 7616, 6750 |
| `cache` | `CacheControl`, `Age`, `Expires` | RFC 9111 |
| `conditional` | `IfMatch`, `IfNoneMatch`, `IfModifiedSince`, `IfUnmodifiedSince`, `IfRange` | RFC 9110 |
| `content_disposition` | `ContentDisposition`, `DispositionType` | RFC 6266 |
| `content_encoding` | `ContentEncoding` | RFC 9110 |
| `content_language` | `ContentLanguage` | RFC 9110 |
| `content_location` | `ContentLocation` | RFC 9110 |
| `content_type` | `ContentType` | RFC 9110 |
| `cookie` | `Cookie`, `SetCookie`, `SameSite` | RFC 6265 |
| `date` | `HttpDate` (IMF-fixdate / asctime は `parse`、rfc850-date は `parse_rfc850(input, reference_year)`), `DateError` | RFC 9110 |
| `digest_fields` | `ContentDigest`, `ReprDigest`, `WantContentDigest`, `WantReprDigest` | RFC 9530 |
| `etag` | `EntityTag`, `ETagList` | RFC 9110 |
| `expect` | `Expect` | RFC 9110 |
| `host` | `Host` (IPv4, IPv6, IPv-future 対応) | RFC 9110 |
| `multipart` | `MultipartParser` (`with_max_buffer_size`, `feed -> Result<(), MultipartError>`), `MultipartBuilder`, `Part`, `MultipartError` | RFC 7578 |
| `range` | `Range`, `RangeSpec`, `ContentRange`, `AcceptRanges` | RFC 9110 |
| `trailer` | `Trailer` (禁止フィールド検証) | RFC 9110, 9112 |
| `upgrade` | `Upgrade` | RFC 9110 |
| `uri` | `Uri`, `UriError`, `percent_encode()`, `percent_encode_path()`, `percent_encode_query()`, `percent_decode()`, `percent_decode_bytes()`, `resolve()`, `normalize()` | RFC 3986 |
| `vary` | `Vary` | RFC 9110 |

## コード例

### クライアント実装

```rust
use shiguredo_http11::{Request, ResponseDecoder};
use std::io::Read;

// リクエスト作成
let request = Request::new("GET", "/")
    .header("Host", "example.com")
    .header("Connection", "close");
let bytes = request.encode();
// bytes をネットワークに送信...

// レスポンスデコード: 内部バッファに直接 read してコピーを排除
let mut decoder = ResponseDecoder::new();
const READ_CHUNK: usize = 8192;
loop {
    let want = decoder.available_buf().min(READ_CHUNK);
    if want == 0 {
        return Err("decoder buffer full".into());
    }
    let buf = decoder.mut_buf(want)?;
    let n = stream.read(buf)?;
    if n == 0 {
        decoder.advance_buf(0);
        decoder.mark_eof();
        break;
    }
    decoder.advance_buf(n);
    if let Some(response) = decoder.decode()? {
        println!("Status: {}", response.status_code);
        break;
    }
}
```

### サーバー実装

```rust
use shiguredo_http11::{RequestDecoder, Response};
use std::io::Read;

// リクエストデコード: 内部バッファに直接 read してコピーを排除
let mut decoder = RequestDecoder::new();
const READ_CHUNK: usize = 8192;
let request = loop {
    let want = decoder.available_buf().min(READ_CHUNK);
    if want == 0 {
        return Err("decoder buffer full".into());
    }
    let buf = decoder.mut_buf(want)?;
    let n = stream.read(buf)?;
    if n == 0 {
        decoder.advance_buf(0);
        return Err("client disconnected".into());
    }
    decoder.advance_buf(n);
    if let Some(req) = decoder.decode()? {
        break req;
    }
};

// レスポンス作成
let response = Response::new(200, "OK")
    .header("Content-Type", "text/plain")
    .body(b"Hello, World!".to_vec());
let bytes = response.encode();
// bytes をネットワークに送信...
```

### HEAD リクエストの処理

HEAD リクエストへのレスポンスは GET と同じヘッダーを返すがボディは送信しない (RFC 9110 Section 9.3.2)。

```rust
use shiguredo_http11::{Request, Response, ResponseDecoder};

// サーバー側: Response::omit_body() でボディ送信を抑止
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

// クライアント側: ResponseDecoder::set_expect_no_body() でボディなしを通知
let mut decoder = ResponseDecoder::new();
decoder.set_expect_no_body(true);
```

### CONNECT トンネルの処理

CONNECT リクエストへの 2xx レスポンスはトンネルモードに切り替わり、Transfer-Encoding と Content-Length は無視される (`BodyKind::Tunnel`)。

```rust
use shiguredo_http11::ResponseDecoder;

let mut decoder = ResponseDecoder::new();
decoder.set_request_method("CONNECT");
// CONNECT 2xx 応答後のボディは Tunnel として扱われる
```

### 直接書き込み API (`mut_buf` / `advance_buf` / `available_buf`)

OS の `read` 等にデコーダーの内部バッファを直接渡せる API。

- `mut_buf(len) -> Result<&mut [u8], Error>`: 内部バッファ末尾に `len` バイトの書き込み枠 (ゼロ初期化済み) を確保。`max_buffer_size` を超える場合は `BufferOverflow` を返し、バッファ状態は不変。
- `advance_buf(n)`: 直前の `mut_buf` で確保した枠のうち、実際に書き込まれた `n` バイトを確定する。`n = 0` で枠全体を破棄 (EOF や read 失敗時のリセット用)。
- `available_buf() -> usize`: 書き込み可能な残り容量 (`max_buffer_size - 現在のバッファ長`)。`pending` を含めて差し引いた値。

#### `feed` / `feed_unchecked` との使い分け

両者は入力経路の違う別の最適解として共存する。用途で使い分ける:

- **これから書き込む先のバッファが必要なケース** (OS の `read` でソケットから受信する等) は `mut_buf` / `advance_buf`。OS が内部バッファに直接書き込めるので、スタックバッファ → 内部 `Vec<u8>` のコピーが発生しない。
- **既にバイト列が `&[u8]` として手元にあるケース** (io_uring 等の完了通知型 I/O、テスト用バイトリテラル、別経路から受け取ったバイト列の中継等) は `feed` / `feed_unchecked`。`mut_buf` 経由だと「ゼロ初期化 + memcpy」の二段になるが、`feed` は素直に 1 memcpy で済む。

```rust
use shiguredo_http11::ResponseDecoder;

let mut decoder = ResponseDecoder::new();
const READ_CHUNK: usize = 8192;

loop {
    let want = decoder.available_buf().min(READ_CHUNK);
    if want == 0 {
        return Err("decoder buffer full".into());
    }
    let buf = decoder.mut_buf(want)?;
    let n = stream.read(buf)?;
    if n == 0 {
        decoder.advance_buf(0);
        decoder.mark_eof();
        if let Some(response) = decoder.decode()? {
            return Ok(response);
        }
        return Err("Connection closed before response complete".into());
    }
    decoder.advance_buf(n);

    if let Some(response) = decoder.decode()? {
        return Ok(response);
    }
}
```

`mut_buf` 後 `advance_buf` を呼ばずに `feed` / `decode_headers` / `peek_body` / `consume_body` 等を呼ぶと debug ビルドで panic する (誤用検出)。

### ストリーミングボディ処理

```rust
use shiguredo_http11::{RequestDecoder, BodyKind, BodyProgress};
use std::io::Read;

let mut decoder = RequestDecoder::new();
const READ_CHUNK: usize = 8192;

// ヘッダーがそろうまで mut_buf 経由で受信
let (head, body_kind) = loop {
    let want = decoder.available_buf().min(READ_CHUNK);
    let buf = decoder.mut_buf(want)?;
    let n = stream.read(buf)?;
    if n == 0 {
        decoder.advance_buf(0);
        return Err("client disconnected before headers".into());
    }
    decoder.advance_buf(n);
    if let Some(result) = decoder.decode_headers()? {
        break result;
    }
};

// ボディをストリーミングで読み取り
let mut body = Vec::new();
if let BodyKind::ContentLength(_) | BodyKind::Chunked = body_kind {
    'outer: loop {
        loop {
            if let Some(data) = decoder.peek_body() {
                body.extend_from_slice(data);
                let len = data.len();
                match decoder.consume_body(len)? {
                    BodyProgress::Complete { .. } => break 'outer,
                    // NeedData (chunked CRLF 不足) でも内側ループ継続。
                    // 直後の peek_body() は None を返すため progress 分岐に進む。
                    BodyProgress::Advanced | BodyProgress::NeedData => continue,
                }
            }
            // peek_body() が None → 状態機械を進める
            match decoder.progress()? {
                BodyProgress::Complete { .. } => break 'outer,
                BodyProgress::Advanced => continue,
                // バッファ不足: 内側ループを抜けて I/O 読み取りに戻る
                BodyProgress::NeedData => break,
            }
        }

        // 追加データを内部バッファに直接 read
        let want = decoder.available_buf().min(READ_CHUNK);
        let buf = decoder.mut_buf(want)?;
        let n = stream.read(buf)?;
        if n == 0 {
            decoder.advance_buf(0);
            return Err("client disconnected during body".into());
        }
        decoder.advance_buf(n);
    }
}
```

### Chunked Transfer Encoding

```rust
use shiguredo_http11::{Response, encode_chunk};

let response = Response::new(200, "OK")
    .header("Transfer-Encoding", "chunked");

// ヘッダーを送信
let headers = response.encode_headers();
send(&headers);

// チャンクを順次送信
send(&encode_chunk(b"Hello, "));
send(&encode_chunk(b"World!"));
send(&encode_chunk(b"")); // 終端チャンク
```

### Result 版エンコード API

`encode()` / `encode_headers()` は RFC 違反でパニックする。エラーハンドリングが必要な場合は `try_encode()` / `try_encode_headers()` を使う。

```rust
use shiguredo_http11::{Request, EncodeError};

let request = Request::new("GET", "/");  // Host ヘッダーなし
match request.try_encode() {
    Ok(bytes) => { /* 送信 */ }
    Err(EncodeError::MissingHostHeader) => {
        // Host ヘッダーがない (HTTP/1.1 必須)
    }
    Err(e) => eprintln!("encode error: {}", e),
}
```

### rfc850-date のパース

`HttpDate::parse` は IMF-fixdate と asctime のみ受理し、rfc850-date を検出した場合は `DateError::Rfc850Date` を返す。rfc850-date の 2 桁年を解決するには `parse_rfc850(input, reference_year)` を使う (RFC 9110 §5.6.7 の 50 年ルール)。

```rust
use shiguredo_http11::date::{HttpDate, DateError};

let input = "Sunday, 06-Nov-94 08:49:37 GMT";
let date = match HttpDate::parse(input) {
    Ok(d) => d,
    Err(DateError::Rfc850Date) => HttpDate::parse_rfc850(input, 2026)?,
    Err(e) => return Err(e),
};
```

`SetCookie::parse` / `Expires::parse` / `IfModifiedSince::parse` / `IfUnmodifiedSince::parse` / `IfRange::parse` は内部でこのフォールバックを行うため、シグネチャに `reference_year: u16` 引数を取る。

### 圧縮トレイトの実装

```rust
use shiguredo_http11::compression::{Compressor, Decompressor, CompressionStatus, CompressionError};
use shiguredo_http11::{ResponseDecoder, ResponseEncoder};

// 展開器を組み込んだデコーダー
let decoder = ResponseDecoder::with_decompressor(MyGzipDecompressor::new());

// 圧縮器を組み込んだエンコーダー
let mut encoder = ResponseEncoder::with_compressor(MyGzipCompressor::new());

// 展開器と制限を同時に指定
let decoder = RequestDecoder::with_decompressor_and_limits(
    MyGzipDecompressor::new(),
    DecoderLimits::default(),
);
```

## DecoderLimits

| フィールド | デフォルト値 | 説明 |
|-----------|-------------|------|
| `max_buffer_size` | 64KB | 最大バッファサイズ |
| `max_headers_count` | 100 | 最大ヘッダー数 |
| `max_header_line_size` | 8KB | 最大ヘッダー行長 |
| `max_body_size` | 10MB | 最大ボディサイズ |
| `max_chunk_line_size` | 64B | 最大チャンクサイズ行長 (16 進数) |

```rust
use shiguredo_http11::{RequestDecoder, DecoderLimits};

// カスタム制限
let limits = DecoderLimits {
    max_body_size: 100 * 1024 * 1024, // 100MB
    ..DecoderLimits::default()
};
let mut decoder = RequestDecoder::with_limits(limits);

// 制限なし (信頼済み入力やテスト用途のみ。OOM に注意)
let decoder = RequestDecoder::with_limits(DecoderLimits::unlimited());
```

## エラー型

### `Error` (デコード/ランタイムエラー)

| バリアント | 説明 |
|-----------|------|
| `Error::InvalidData(String)` | 不正なデータ |
| `Error::BufferOverflow { size, limit }` | バッファサイズ超過 |
| `Error::TooManyHeaders { count, limit }` | ヘッダー数超過 |
| `Error::HeaderLineTooLong { size, limit }` | ヘッダー行長超過 |
| `Error::BodyTooLarge { size, limit }` | ボディサイズ超過 |
| `Error::ChunkLineTooLong { size, limit }` | チャンクサイズ行長超過 |
| `Error::Compression(CompressionError)` | 圧縮/展開エラー |

### `EncodeError` (エンコード時のバリデーションエラー)

主なバリアント:

| バリアント | 説明 |
|-----------|------|
| `MissingHostHeader` | HTTP/1.1 リクエストに Host ヘッダーがない (RFC 9112 §3.2) |
| `ConflictingTransferEncodingAndContentLength` | Transfer-Encoding と Content-Length 同時指定 (RFC 9112 §6.2) |
| `ForbiddenTransferEncoding { status_code }` | 1xx/204/205 で Transfer-Encoding 指定 (RFC 9112 §6.1, RFC 9110 §15.3.6) |
| `ForbiddenContentLength { status_code }` | 1xx/204 で Content-Length 指定 (RFC 9110 §8.6) |
| `ForbiddenBodyFor205` | 205 Reset Content にボディあり (RFC 9110 §15.3.6) |
| `ContentLengthMismatch { header_value, body_length }` | Content-Length とボディ長が不一致 |
| `DuplicateContentLength` | Content-Length が複数で値が不一致 (RFC 9110 §8.6) |
| `InvalidContentLengthValue { value }` | Content-Length が `1*DIGIT` ABNF 違反 |
| `InvalidMethod { method }` | 不正なメソッド名 |
| `InvalidRequestTarget { uri }` | 不正な request-target |
| `InvalidRequestTargetForm { method, uri }` | メソッドと request-target 形式の不整合 (RFC 9112 §3.2) |
| `InvalidVersion { version }` | 不正な HTTP バージョン |
| `InvalidHeaderName { name }` | 不正なヘッダー名 |
| `InvalidHeaderValue { name, value }` | 不正なヘッダー値 |
| `InvalidStatusCode { code }` | 不正なステータスコード |
| `InvalidReasonPhrase { phrase }` | 不正な reason-phrase |
| `DuplicateHostHeader` | Host ヘッダーが重複 |
| `InvalidHostHeader { value }` | 不正な Host ヘッダー値 |
| `HostAuthorityMismatch { host, authority }` | Host と request-target authority が不一致 |
| `UserinfoInHttpUri { uri }` | http/https URI に userinfo (RFC 9110 §4.2.4) |
| `NonEmptyHostWithoutAuthority { host, uri }` | authority のない URI で Host が非空 (RFC 9112 §3.2) |
| `EmptyHostInHttpUri { uri }` | http/https URI の host が空 (RFC 9110 §4.2) |

## セキュリティ

- **Response Splitting 対策**: ヘッダー行の CR/LF と obs-fold を拒否
- **Request Smuggling 対策**: Transfer-Encoding と Content-Length の同時指定を拒否
- **Request Smuggling 対策**: Content-Length の不一致検出
- **DoS 対策**: `DecoderLimits` による各種制限
- **CONNECT の扱い**: RFC 9110 §9.3.6 に準拠

## RFC 準拠

| RFC | 名称 | 対応機能 |
|-----|------|---------|
| RFC 3986 | URI: Generic Syntax | URI パース、パーセントエンコーディング、resolve, normalize |
| RFC 6265 | HTTP State Management | Cookie, Set-Cookie |
| RFC 6266 | Content-Disposition | ファイル添付 |
| RFC 6750 | Bearer Token | Bearer 認証 |
| RFC 7578 | multipart/form-data | フォームデータ |
| RFC 7616 | Digest Auth | Digest 認証 |
| RFC 7617 | Basic Auth | Basic 認証 |
| RFC 8187 | Encoded HTTP Header Parameters | filename* 等のエンコード |
| RFC 9110 | HTTP Semantics | メソッド、ステータス、ヘッダー、条件付きリクエスト、Range |
| RFC 9111 | HTTP Caching | Cache-Control, Age, Expires |
| RFC 9112 | HTTP/1.1 | Transfer-Encoding, Content-Length, 接続管理, request-target 形式 |
| RFC 9530 | Digest Fields | Content-Digest, Repr-Digest, Want-Content-Digest, Want-Repr-Digest |

## 既知の制限事項

- **UTF-8 強制**: RFC 9112 §2.2 ではメッセージをオクテット列として解析すべき (SHOULD) としているが、本実装はチャンクサイズ行・トレーラー・ヘッダー値などを Rust の `String` として扱うため、非 UTF-8 バイト列 (obs-text 0x80-0xFF を含む) を拒否する。
- **request-line 前の空行**: RFC 9112 §2.2 では先頭の空行を少なくとも 1 行は無視すべき (SHOULD) としているが、本実装は厳格にパースする。アプリケーション層で除去すること。
- **rfc850-date の reference_year**: グローバル可変状態を持たないため、rfc850-date を含む可能性があるヘッダーをパースする API (`SetCookie::parse` 等) は `reference_year: u16` 引数を要求する。アプリケーション側で現在年を渡す責務を負う。
