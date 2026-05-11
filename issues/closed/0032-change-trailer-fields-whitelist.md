# 0032: trailer フィールドの受理判定をホワイトリスト方式に変更する

Created: 2026-05-12
Completed: 2026-05-12
Model: Opus 4.7

## 概要

`src/trailer.rs::is_prohibited_trailer_field` は trailer に含めてはならないフィールドの**ブロックリスト**を持っているが、現状のリストは framing / routing / content format の一部 (Transfer-Encoding, Content-Length, Host, Trailer, Content-Encoding, Content-Type, Content-Range) のみで、RFC 9110 Section 6.5.1 が要求するカテゴリを網羅していない。コメントでもこのことを自認している。

特に以下のカテゴリのフィールドが trailer 経由で素通りする:

- 認証系: `Authorization`, `Proxy-Authorization`, `WWW-Authenticate`, `Proxy-Authenticate`
- リクエスト修飾子: `If-Match`, `If-None-Match`, `If-Modified-Since`, `If-Unmodified-Since`, `If-Range`, `Range`, `Expect`, `TE`
- レスポンス制御: `Cache-Control`, `Vary`, `Date`, `Expires`, `Age`, `Set-Cookie`
- 接続: `Connection`, `Upgrade`

RFC 9110 Section 6.5.1 は:

> A sender MUST NOT generate a trailer field unless the sender knows the corresponding header field name's definition permits the field to be sent in trailers.

と規定しており、受信側も同等に厳格に扱うべき。本 issue では「`Trailer:` ヘッダーで sender が事前申告した名前のみ受理する**ホワイトリスト方式**」に変更する。

## 根拠

### RFC

- RFC 9110 Section 6.5.1: 「A sender MUST NOT generate a trailer field unless the sender knows the corresponding header field name's definition permits the field to be sent in trailers.」
- RFC 9112 Section 7.1.2: 「When a chunked message containing a non-empty trailer section is received, ... the user agent SHOULD ... discard any received trailer fields, or ... store the received trailer fields as if they were received as header fields.」「the recipient SHOULD ... ignore (discard) the trailers」

### 攻撃シナリオ

1. 攻撃者が `Trailer: X-Custom\r\n` で `X-Custom` のみを事前申告
2. 実際の trailer-section で `Authorization: Bearer ...\r\n` を送る (申告されていないフィールド)
3. 受信側がブロックリスト方式だと `Authorization` は禁止リストにないので素通り
4. 上位アプリケーションが trailer-section の `Authorization` を「あとから上書きされた認証ヘッダー」として処理 → 認証バイパス

ホワイトリスト方式 (申告されたフィールドのみ受理) ならこの攻撃を遮断できる。

## 対応方針

### `src/trailer.rs`

- `is_prohibited_trailer_field` のリストを RFC 9110 Section 6.5.1 のカテゴリ全網羅に拡充する:
  - framing: `Transfer-Encoding`, `Content-Length`
  - routing: `Host`
  - リクエスト修飾子: `If-Match`, `If-None-Match`, `If-Modified-Since`, `If-Unmodified-Since`, `If-Range`, `Range`, `Expect`, `TE`
  - 認証: `Authorization`, `Proxy-Authorization`, `WWW-Authenticate`, `Proxy-Authenticate`
  - レスポンス制御: `Cache-Control`, `Vary`, `Date`, `Expires`, `Age`, `Set-Cookie`
  - コンテンツ形式: `Content-Encoding`, `Content-Type`, `Content-Range`
  - 接続管理: `Connection`, `Upgrade`, `Trailer` (Trailer 自身も trailer-section に置けない)
- `Trailer::parse(input, declared: Option<&[&str]>)` のような形でホワイトリスト方式を導入するか、または `Trailer` 構造体に「申告された名前リスト」を保持させる
- 既存テストの「`prohibited_field_*`」シリーズを拡充カテゴリに対応

### `src/decoder/body.rs::process_trailers`

- `BodyDecoder` に `declared_trailers: Vec<String>` フィールド (lowercase) を追加
- `BodyDecoder::set_declared_trailers(&mut self, declared: Vec<String>)` メソッドを追加
- `reset()` でクリアする
- `process_trailers` で受信した trailer 名について:
  1. `is_prohibited_trailer_field` でカテゴリ違反 → reject (従来通り、ただしリスト拡充)
  2. `declared_trailers` に含まれない名前 → reject (新ロジック)

### `src/decoder/request.rs::decode_headers` / `src/decoder/response.rs::decode_headers`

- ヘッダー完了直後、`Trailer:` ヘッダーから値を抽出して lowercase 化したリストを `BodyDecoder::set_declared_trailers` に渡す
- `Trailer:` ヘッダーがない場合は空リスト = trailer は一切受理しない

### テスト

- `tests/test_decoder.rs` (or `tests/test_trailer.rs` を新設):
  - `Trailer: X-Custom\r\n` を申告したリクエスト/レスポンスで trailer-section に `X-Custom` が来れば受理
  - 同条件で `Authorization` が trailer-section に来たら reject (認証バイパス防御)
  - 同条件で `X-Other` (申告されていない) が来たら reject (ホワイトリスト動作)
  - `Trailer:` ヘッダーがない場合、trailer-section に何が来ても reject
- `src/trailer.rs` の inline test に拡充されたカテゴリのテストを追加

### CHANGES.md

`## develop` のメインに `[CHANGE]` として追記。受理範囲が縮小するため後方互換性なし。

### 破壊的変更

- `Trailer:` ヘッダーで予告されていない trailer フィールドはすべて拒否される
- 拡充カテゴリのフィールドも常に拒否される
- canary リリース中なので破壊的変更は許容範囲

## 解決方法

- `src/trailer.rs::is_prohibited_trailer_field` のリストを RFC 9110 Section 6.5.1 全カテゴリに拡充した:
  - framing: `Transfer-Encoding`, `Content-Length`
  - routing: `Host`
  - リクエスト修飾子: `If-Match`, `If-None-Match`, `If-Modified-Since`, `If-Unmodified-Since`, `If-Range`, `Range`, `Expect`, `TE`
  - 認証: `Authorization`, `Proxy-Authorization`, `WWW-Authenticate`, `Proxy-Authenticate`
  - レスポンス制御: `Cache-Control`, `Vary`, `Date`, `Expires`, `Age`, `Set-Cookie`
  - コンテンツ形式: `Content-Encoding`, `Content-Type`, `Content-Range`
  - 接続管理: `Connection`, `Upgrade`, `Trailer`
- `src/decoder/body.rs::BodyDecoder` に `declared_trailers: Vec<String>` フィールドと `set_declared_trailers(declared)` メソッドを追加した。`reset()` でクリアする
- `src/decoder/body.rs` に `collect_declared_trailers(headers)` ヘルパーを追加。`Trailer:` ヘッダー値をカンマ区切りで分解し ASCII 小文字化したリストを返す
- `src/decoder/body.rs::process_trailers` でホワイトリスト判定を追加: 受信した trailer 名が `declared_trailers` に含まれない場合は `Error::InvalidData("undeclared trailer field: ...")` を返す。カテゴリ違反 (拡充された `is_prohibited_trailer_field`) チェックも引き続き行う
- `src/decoder/request.rs::decode_headers` / `src/decoder/response.rs::decode_headers`: ヘッダー完了直後に `collect_declared_trailers(&self.headers)` を呼び `body_decoder.set_declared_trailers(declared)` で渡す
- テスト:
  - `src/trailer.rs` inline test: 拡充カテゴリ (認証 / リクエスト修飾子 / レスポンス制御 / 接続管理) の禁止判定を網羅。旧テストで `Expires` を「許可フィールド」として使っていたものを `X-Test` 等の拡張ヘッダー名に置き換え
  - `tests/test_decoder.rs`:
    - `test_chunked_trailer_too_many_error` / `test_chunked_trailer_line_too_long_error`: `Trailer:` ヘッダーで事前申告するよう修正
    - 新規 `test_chunked_trailer_whitelist_accepts_declared_field`: 申告された trailer が受理されることを確認
    - 新規 `test_chunked_trailer_whitelist_rejects_undeclared_field`: 申告されていない trailer が reject されることを確認
    - 新規 `test_chunked_trailer_whitelist_rejects_authorization_injection`: 認証ヘッダー後付け注入による smuggling を遮断することを確認
    - 新規 `test_chunked_trailer_whitelist_rejects_unannounced_trailers`: `Trailer:` ヘッダー無しの場合は trailer-section を一切受理しないことを確認
  - `pbt/tests/prop_decoder/body.rs`: `prop_chunked_with_trailer_ok` / `prop_chunked_with_multiple_trailers_ok` で `Trailer:` ヘッダーを生成して事前申告するよう修正
- `src/trailer.rs` の lib doc サンプル (`Trailer::parse("Expires, X-Test")`) を `X-Checksum, X-Test` に変更し、拡充カテゴリと一貫させた
- `CHANGES.md` の `## develop` 先頭に `[CHANGE]` エントリを追加した
