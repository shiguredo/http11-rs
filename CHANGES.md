# 変更履歴

- UPDATE
  - 後方互換がある変更
- ADD
  - 後方互換がある追加
- CHANGE
  - 後方互換のない変更
- FIX
  - バグ修正

## develop

- [UPDATE] `MultipartParser` のバッファ管理を読み取り位置オフセット方式に変更する
  - 多数パートの multipart ボディに対するコピー量を `O(N²)` から amortized `O(N)` に改善する
  - boundary 文字列のデリミタを `MultipartParser::new()` で事前計算してフィールドに持ち、`next_part()` ごとの `format!` を除去する
  - @voluntas
- [UPDATE] `encode_chunk` / `encode_chunks` のチャンクサイズ生成からヒープ確保を除去する
  - 16 進数文字列の生成にスタックバッファを使う `write_hex_usize` ヘルパーを導入し、ストリーミング送信時の `format!` を除去する
  - 併せて `encode_request` / `encode_response` / `encode_response_headers` のステータスコード / Content-Length の `to_string()` を `write_usize_decimal` ヘルパーに置き換える
  - `encode_chunk` / `encode_chunks` のバッファを `Vec::with_capacity` に変更し、`checked_add` ベースで容量を見積もる
  - @voluntas
- [UPDATE] `encode_request` / `encode_response` のバッファに `Vec::with_capacity` を導入する
  - 容量見積もりを `checked_add` ベースで行い、オーバーフロー時は `Vec::new()` にフォールバックする
  - `ENCODE_CAPACITY_LIMIT` (64 MB) を導入し、攻撃者制御のヘッダー値による `Vec::with_capacity` の OOM abort を防ぐ
  - 自動付与する Content-Length 行と auto-emit 判定ロジックは見積もり / 書き込み双方で共通の関数 (`should_auto_emit_content_length_for_request` / `..._for_response`) を経由するように整理する
  - 任意入力でのパニック / abort 安全性を `fuzz_encode_request` / `fuzz_encode_response` で網羅する
  - @voluntas
- [ADD] `ResponseDecoder` / `RequestDecoder` に直接書き込み API (`mut_buf` / `advance_buf` / `available_buf`) を追加する
  - OS の `recv()` 等がデコーダー内部バッファに直接書き込めるようにし、`feed(&[u8])` 経由の中間コピーを排除する
  - `available_buf()` で残容量を問い合わせてチャンクサイズを適応させる
  - `examples/http11_client` / `examples/http11_server` / `examples/http11_reverse_proxy` の受信ループを新 API に書き換える
  - @voluntas

### misc

- [UPDATE] `examples/http11_client` をストリーミング API の実装例に書き換える
  - `decode()` 一括 API から `decode_headers()` + `peek_body()` / `consume_body()` / `progress()` に変更する
  - `Instant` で TTFB / first-body-byte / total の各タイミングを計測して `tracing::info!` で出力する
  - 全 `BodyKind` (Chunked / ContentLength / CloseDelimited / None / Tunnel) に対応する
  - @voluntas
- [UPDATE] `examples/http11_server` をストリーミング API の実装例に書き換える
  - `while let Some(request) = decoder.decode()?` を `decode_headers()` + ストリーミングボディ受信に変更する
  - `StreamingState` / `stream_body()` / `serve_request()` で Keep-Alive 対応を維持しつつコードを整理する
  - @voluntas

## 2026.2.0

**リリース日**: 2026-04-30

- [UPDATE] `Request` と `Response` に `HttpHead` トレイトを実装する
  - `get_header` / `get_headers` / `has_header` / `connection` / `is_keep_alive` / `content_length` / `is_chunked` を `HttpHead` デフォルト実装に委譲する
  - 重複していた 120 行以上の同一ロジックを統一する
  - @voluntas
- [UPDATE] `is_valid_request_target()` と `encoder.rs` のコメントを整備する
  - obs-text の扱いについて、受信側の寛容さと送信側の拒否という責務を明確にする
  - @voluntas
- [UPDATE] `HttpHead::is_keep_alive()` / `is_chunked()` の内部実装を `headers().iter()` で直接走査するように変更する
  - `get_headers()` を経由しないことで呼び出し時の不要な `Vec<&str>` allocation を回避する
  - `get_headers()` / `is_keep_alive()` / `is_chunked()` のシグネチャは変更せず、object safe を維持する
  - @voluntas
- [UPDATE] `src/lib.rs` の `#[macro_use] extern crate alloc` から `#[macro_use]` を削除する
  - `vec!` / `format!` を使っていた通常コードを `alloc::vec!` / `alloc::format!` に置換する
  - `no_std` 環境でのマクロの使い方を明示的にする
  - @voluntas
- [ADD] `MultipartParser` にバッファ上限を追加する
  - `max_buffer_size` フィールドを追加し、デフォルト 10MB の上限を設ける
  - `with_max_buffer_size()` ビルダーメソッドを追加する
  - @voluntas
- [ADD] `feed_unchecked()` と `DecoderLimits::unlimited()` に未信頼入力での OOM リスクを警告するドキュメントを追加する
  - @voluntas
- [CHANGE] `src` を `core` と `alloc` のみの `no_std` に対応する
  - `#![no_std]` を宣言し、`std` への依存を排除する
  - @voluntas
- [CHANGE] `HttpDate` の API を obs-date 対応のために再設計する
  - `HttpDate::parse(&str)` は IMF-fixdate と asctime のみを受理する
  - rfc850-date を検出した場合は `Err(DateError::Rfc850Date)` を返し、`HttpDate::parse_rfc850(&str, reference_year: u16)` でフォールバックする設計とする
  - `HttpDate::parse_rfc850` は ABNF 通りの 2 桁年に加え、Postel 原則で 4 桁年も受理する (4 桁年の場合 `reference_year` は使用されない)
  - 2 桁年は RFC 9110 §5.6.7 の 50 年ルールで `reference_year` を基準に解決する
  - グローバル可変状態 (`AtomicU16` による暗黙の参照年) と `set_http_date_reference_year` 関数を完全に削除し、no_std でも安全に扱えるようにする
  - `DateError::Rfc850Date` バリアントを追加する
  - @voluntas
- [CHANGE] `SetCookie::parse` / `Expires::parse` / `IfModifiedSince::parse` / `IfUnmodifiedSince::parse` / `IfRange::parse` のシグネチャに `reference_year: u16` 引数を追加する
  - RFC 9110 §5.6.7 が要求する 3 形式 (IMF-fixdate / rfc850-date / asctime) すべての受理を満たすために必要
  - 内部で `HttpDate::parse` → `Rfc850Date` エラー時に `HttpDate::parse_rfc850` へフォールバックする
  - @voluntas
- [CHANGE] `MultipartParser::feed()` の戻り値を `Result<(), MultipartError>` に変更する
  - バッファ上限超過時に `MultipartError::BufferOverflow` を返す
  - @voluntas
- [CHANGE] `RequestTargetForm` を `decoder::body` から `request_target` モジュールに移動する
  - `decoder::body::RequestTargetForm` から `request_target::RequestTargetForm` へインポートパスを変更する
  - encoder と decoder で重複していた定義を統一する
  - @voluntas
- [CHANGE] `Content-Length` の型を `usize` から `u64` に変更する
  - `Request::content_length()` / `Response::content_length()` / `HttpHead::content_length()` の戻り値を `Option<u64>` に変更する
  - `BodyKind::ContentLength` の型を `u64` に変更する
  - `DecodePhase::BodyContentLength { remaining: u64 }` に変更する
  - 32bit 環境での integer conversion overflow と precision loss を防ぐ (RFC 9110 Section 8.6)
  - @voluntas
- [FIX] `MultipartParser::feed()` のバッファサイズ計算で整数オーバーフローによる panic を回避する
  - @voluntas

### misc

- [UPDATE] README に Agent Skills のインストール方法を追記する
  - `gh skill install shiguredo/http11-rs shiguredo-http11` でインストールできる旨を記載する
  - @voluntas
- [UPDATE] README の `BodyKind::ContentLength` の型表記を `usize` から `u64` に修正する
  - 2026.2.0 で型を `u64` に変更したことに合わせる
  - @voluntas
- [UPDATE] `src/auth.rs` と `src/digest_fields.rs` に重複していた Base64 エンコード/デコード実装を `src/base64.rs` に共通化する
  - @voluntas
- [UPDATE] `examples/` の gzip 圧縮/展開を `flate2` から `noflate` に切り替える
  - `http11_client` の `decompress_body` を `noflate::gzip::decompress` に置き換える
  - `http11_server` / `http11_server_io_uring` の `GzipCompressor` を `noflate::gzip::Encoder` の sans-io API ベースに書き換え、`compress_body` を `noflate::gzip::compress` に置き換える
  - `noflate` には圧縮レベル概念がないため未使用だった `GzipCompressor::with_level` を削除する
  - @voluntas
- [UPDATE] `src/validate.rs` のエンコード専用ポリシー `is_valid_version_for_encode` を `src/encoder.rs` に移動する
  - `src/validate.rs` を RFC 9110 / RFC 3986 基本文字集合の共通検証に特化させ、モジュール責務を明確にする
  - @voluntas

## 2026.1.1

**リリース日**: 2026-03-16

- [FIX] CONNECT リクエストの Content-Length / Transfer-Encoding の扱いを RFC 9110 Section 9.3.6 に準拠させる
  - ヘッダーが存在するだけで reject しないようにする (RFC は MUST NOT としていない)
  - ヘッダーが存在しても body として読まず BodyKind::None として扱うようにする ("does not have content")
  - @voluntas

## 2026.1.0

**リリース日**: 2026-02-25

**公開**
