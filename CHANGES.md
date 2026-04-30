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
