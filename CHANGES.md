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
- [ADD] `MultipartParser` にバッファ上限を追加する
  - `max_buffer_size` フィールドを追加し、デフォルト 10MB の上限を設ける
  - `with_max_buffer_size()` ビルダーメソッドを追加する
  - @voluntas

### misc

- [ADD] `feed_unchecked()` と `DecoderLimits::unlimited()` に未信頼入力での OOM リスクを警告するドキュメントを追加する

## 2026.1.1

**リリース日**: 2026-03-16

- [FIX] CONNECT リクエストの Content-Length / Transfer-Encoding の扱いを RFC 9110 Section 9.3.6 に準拠させる
  - ヘッダーが存在するだけで reject しないようにする (RFC は MUST NOT としていない)
  - ヘッダーが存在しても body として読まず BodyKind::None として扱うようにする ("does not have content")
  - @voluntas

## 2026.1.0

**リリース日**: 2026-02-25

**公開**
