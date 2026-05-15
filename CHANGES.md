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

- [CHANGE] `SetCookie::parse` の `Domain` 属性を RFC 1034 subdomain 構文 (LDH + dot) 準拠で厳格化する
  - `Domain=..` / `Domain=...` / `Domain=..foo` のような leading dot 複数の入力を `None` 扱いに変更する (旧 `Some(".")` / `Some("..")` / `Some(".foo")`)
  - `Domain=foo bar` / `Domain=foo\0bar` / `Domain=. foo` / `Domain=日本.example` のような非 LDH 文字 (空白・NUL・制御文字・非 ASCII) を含む入力も `None` 扱いに変更する
  - 旧実装は RFC 6265 Section 5.2.3 の strip 規則 (先頭 dot を 1 つ削除) を素直に実装し strip 後の値の validity を検証していなかったため、`Domain=..` が `Some(".")`、`Domain=. foo` が `Some(" foo")` (leading space) で保存され、`Display` 出力を再 parse すると別値に縮退して `parse -> to_string -> parse` の roundtrip が破綻していた (fuzz_cookie の 2 系統の crash)
  - RFC 6265 Section 4.1.1 の `domain-value = <subdomain>` (RFC 1034 Section 3.5 + RFC 1123 Section 2.1) と RFC 6265bis Section 6.3 (IDN は punycode = LDH 必須) に従い、strip 後の値が LDH + dot のみで構成されていることを検証する。RFC 6265 の strip 規則自体 (1 つだけ strip) は変更しない
  - @voluntas
- [CHANGE] `AcceptError` / `ExpectError` に `UnterminatedQuote` バリアントを追加する
  - 旧実装は閉じ DQUOTE が無い quoted-string をそれぞれ `InvalidParameter` / `InvalidValue` に潰していたが、`ContentTypeError::UnterminatedQuote` と粒度を揃えて構造エラーと文字種エラーを区別する
  - `From<QuotedStringError>` impl 経由で 3 モジュール共通の `validate::parse_quoted_string` から各エラー型へマップする
  - `#[non_exhaustive]` のため利用側の `match` で warning が出る場合は新バリアントを追加してハンドリングすること
  - @voluntas
- [CHANGE] `Accept` / `Content-Type` の `Display` で値が空文字列のパラメータを `name=""` (引用符付き) で出力する
  - 旧実装は `needs_quoting("")` が `false` を返したため `name=` (引用符なし) を出力し、再 parse すると `is_valid_token("")` が `false` で reject される Display ラウンドトリップ破綻があった
  - RFC 9110 Section 5.6.2 の `token = 1*tchar` (空文字 token は構文上不可) に従い、空値は必ず引用符付き (`name=""`) で出力する
  - @voluntas
- [FIX] `Accept` / `Content-Type` / `Expect` の quoted-string パースに qdtext / quoted-pair 文字種検証を追加する
  - 旧実装は RFC 9110 Section 5.5 の CR/LF/NUL MUST 要件に違反し、任意の制御文字 (CR / LF / NUL / 他の CTL) を無条件に受理していた
  - 受理された制御文字が上位アプリで再エンコードされると HTTP Response Splitting (CWE-113) / log injection の経路となる
  - 修正後は CR / LF / NUL / 他の CTL (%x01-08, %x0B-0C, %x0E-1F, %x7F DEL) を含む quoted-string は qdtext / quoted-pair のどちらの右辺でも reject される (これまで通っていた入力が `InvalidParameter` / `InvalidValue` を返すようになる)
  - 共通実装を `src/validate.rs` の `parse_quoted_string` (`pub(crate)`) に集約し、3 モジュールから `From<QuotedStringError>` 経由で呼び出す
  - obs-text (U+0080 以上) は opaque data として引き続き受理する (RFC 9110 Section 5.5)
  - @voluntas
- [FIX] `Authorization` / `Content-Disposition` の quoted-string パースで obs-text を含む UTF-8 値の Latin-1 mojibake を修正する
  - 旧実装は入力 `&str` を `as_bytes()` で 1 バイトずつ走査し `b as char` で `String` に push していたため、UTF-8 マルチバイトシーケンスが `U+0080..=U+00FF` にマップされ Display 出力で別バイトに展開、ラウンドトリップで mojibake していた
  - char 単位走査に書き換え、入力 `&str` の UTF-8 不変条件を保つ
  - obs-text は RFC 9110 Section 5.5 の「recipient SHOULD treat obs-text as opaque data」に従い opaque な char として保持する (reject しない)。CR / LF / NUL の reject は char 版ヘルパー `is_qdtext_char` / `is_quoted_pair_char` で等価に維持する
  - issue 0036 で導入した `is_qdtext_byte` / `is_quoted_pair_byte` (`pub(crate)`、2026.4.0 リリース済) を char 版に置き換え本体を削除する
  - @voluntas
- [FIX] URI の `normalize` で path-only URI が network-path reference や scheme 付き URI に化けて冪等性が破れる不具合を修正する
  - 旧実装は `build_uri` が「authority なし、path が `//` 始まり」の文字列を構成しており、再 parse で authority に化け、再度 normalize すると host が小文字化されて結果が変わっていた (RFC 3986 Section 3.3 違反)。`build_uri` で `authority.is_none() && path.starts_with("//")` のとき path 先頭に `/.` を挿入するように修正する
  - 関連して、`build_uri` が「scheme なし、authority なし、path の最初の segment に `:` を含む」文字列を出力すると、再 parse で先頭 segment が scheme として誤解釈されていた (RFC 3986 Section 4.2 違反)。同じく `build_uri` で `./` を path 先頭に挿入するように修正する
  - `normalize` の処理順を RFC 3986 Section 6.2.2 通り「percent-encoding 正規化 (6.2.2.2)」→「dot-segment 除去 (6.2.2.3)」の順に修正する。旧実装は逆順だったため、`%2E` (= `.`) のような encoded dot が dot-segment 除去後に decode され、結果として残った `/./` が次回 normalize で除去されて冪等性が崩れていた
  - @voluntas
- [FIX] `Connection` / `Transfer-Encoding` のトークン OWS 除去で `str::trim()` を `trim_ows` に統一し Unicode 空白による HTTP Request Smuggling 経路を塞ぐ
  - 旧実装は `src/decoder/head.rs::is_keep_alive` / `is_chunked` で `str::trim()` を使用しており、NBSP (U+00A0) 等の Unicode 空白を除去していた
  - `is_valid_field_value` は obs-text (0x80-0xFF) を許容するため、前段プロキシ (ASCII OWS のみ trim) との解釈不一致で HTTP Request Smuggling (CWE-444) の足場となっていた
  - 併せて encoder の 205 Content-Length 検証も `trim_ows` に置換し防御層の一貫性を確保する
  - @voluntas
- [FIX] `ContentRange::length()` の整数オーバーフローを修正し `new_bytes()` にバリデーションを追加する
  - `length()` が `e - s + 1` を unchecked に計算しており、`(0, u64::MAX)` で debug ビルドの panic / release ビルドの wrapping を起こしていた
  - `checked_sub` + `checked_add` に変更し、オーバーフロー時は `None` を返す
  - `new_bytes()` に `start > end` と `complete_length <= end` の `assert!` を追加する (RFC 9110 Section 14.4 の validity rule)
  - `parse()` の検証ロジックを `validate_content_range_parts()` として抽出し重複を除去する
  - @voluntas
- [FIX] `escape_quotes()` の CTL 検出を `debug_assert!` から常時有効な SP 置換に変更する
  - 旧実装は `debug_assert!` で CTL 検出を行っており、release ビルドでは CTL 文字が素通りして HTTP Response Splitting (CWE-113) の経路になっていた
  - `is_quoted_pair_char` に適合しない文字 (CR / LF / NUL / 他の CTL / DEL) を SP に置換する (RFC 9110 Section 5.5 "MUST either reject or replace with SP")
  - @voluntas
- [FIX] Keep-Alive 接続で RequestDecoder / ResponseDecoder の Decompressor をリセットし状態漏れを防ぐ
  - `decode()` 完了時と `decode_headers()` Complete→StartLine 遷移時の 4 箇所に `self.decompressor.reset()` を追加する
  - 前メッセージの Decompressor 内部状態が後続メッセージに持ち越されるデータ破損経路を塞ぐ
  - @voluntas
- [FIX] `HttpDate::new` に月別日数検証を追加し、無効な日付の構築を防ぐ
  - `day` の検証を `1..=31` 固定から `max_day_in_month(month, year)` に変更する
  - 2 月のうるう年判定を含む (RFC 9110 Section 5.6.7 IMF-fixdate)
  - @voluntas
- [FIX] `MultipartParser` の `InPart` / `AfterInnerDelimiter` 状態で transport-padding に対応する
  - 内部デリミタ `\r\n--<boundary>` 直後に SP/HTAB の transport-padding をスキップする (RFC 2046 Section 5.1.1)
  - padding 途中で buffer が尽きた場合は `AfterInnerDelimiter` に留まり次回 feed で再開する
  - @voluntas
- [FIX] `encode_request_headers` / `encode_response_headers` に Content-Length の値検証と body 長整合性検証を追加する
  - `encode_response_headers` の `debug_assert!` ブロックを常時有効な検証に変更し release ビルドでも防御する
  - @voluntas

### misc

- [UPDATE] `detect_scheme` を `request_target.rs` に一元化し encoder/decoder 間の重複を除去する
  - `src/encoder.rs` と `src/decoder/body.rs` の重複定義を削除し `crate::request_target::detect_scheme` に集約する
  - @voluntas
- [FIX] `examples/http11_client` のボディ出力で UTF-8 文字境界パニックを修正する
  - `&text[..1000]` を `&text[..text.floor_char_boundary(1000)]` に変更し安全な truncate にする
  - @voluntas
- [FIX] `examples/http11_server` / `examples/http11_server_io_uring` の Accept-Encoding qvalue デフォルト値を RFC 9110 Section 12.4.2 に準拠させる
  - `unwrap_or(1.0)` を `unwrap_or(0.0)` に修正する
  - @voluntas
- [FIX] `parse_content_length_value` と `parse_dictionary` の空カンマ区切り要素を RFC 9110 Section 5.6.1.2 に従いスキップする
  - 空リスト要素をエラーではなく ignore する
  - 全要素が空の場合は引き続きエラーとする
  - @voluntas

### misc

- [UPDATE] (crate 内部) `is_valid_reason_phrase` を RFC 9112 Section 4 ABNF `1*(HTAB / SP / VCHAR / obs-text)` に厳密準拠させ、空文字列を非合法と判定する
  - decoder / encoder の呼び出し側で reason-phrase absent (空文字列) はスキップする方式に統一する
  - reverse proxy 等の経路で「decoder が受理した空 reason_phrase の Response を encoder で再送信する」ケースを RFC 9112 Section 4 に準拠したまま透過できる
  - @voluntas
- [UPDATE] `Response`、`Request`、`HttpHead` の委譲メソッドの doc に RFC 節番号を明記する
  - 対象メソッド: `omit_body` / `is_body_omitted` / `is_keep_alive` / `is_chunked` / `content_length` / `connection`
  - `is_keep_alive` の doc に判定ロジックの全体像 (`close` 優先、version フォールバック) を明記し、RFC 9112 Section 9.6 (close option) の参照を追加する
  - `content_length` の doc に「最初のヘッダー値のみを返す」挙動と RFC 9110 Section 17.5 の整数変換オーバーフロー防止要件を明記する
  - `connection` の doc に「そのままの &str で返し、トークン分割は行わない」挙動を明記する
  - `omit_body` フィールドに RFC 9110 Section 9.3.2 / Section 6.4.1 の参照を追加し、1xx/204/304 はエンコーダーが自動抑止するため不要であることも明記する
  - @voluntas
- [UPDATE] CI を `ci` (全 OS) と `e2e-test` (ubuntu-24.04 のみ) の 2 job に分割し、外部依存 (curl / Docker) を持つ examples の integration test を Linux runner でのみ実行する
  - 既存 `ci` job は `cargo test --workspace --exclude http11_client --exclude http11_server` に変更し、macos / windows での Docker 不在 (testcontainers) や OS 差異による fail を回避する
  - 新規 `e2e-test` job が `cargo test -p http11_client -p http11_server` を担当する
  - @voluntas
- [UPDATE] `examples/http11_client` の `Decompressor` トレイト実装を完成させ、レスポンスボディをストリーミング展開する形に書き換える (issue 0028)
  - `decompressor.rs` の `GzipDecompressor` / `BrotliDecompressor` / `ZstdDecompressor` を各 crate のストリーミング API (noflate `gzip::Decoder` / `BrotliDecompressStream` + `BrotliState` / `zstd::stream::raw::Decoder`) のラッパーとして実装する
  - Content-Encoding ヘッダー受信後に展開器の種別を確定する用途向けに `AnyDecompressor` enum を新設する (variant サイズ差を抑えるため `Box` 経由で持つ)
  - `transport.rs` を `peek_body()` + `AnyDecompressor::decompress` の手動連携経路に書き換え、1 GiB のボディでも 8 KiB 出力バッファでストリーミング展開できる構成にする
  - `src/main.rs` の `print_response` から一括展開関数 `decompress_body` の呼び出しを削除する (受信時点で既に展開済み)
  - `tests/nginx_streaming.rs` に `streams_large_gzip_body` (transport.rs 経路) と `peek_body_decompressed_streams_gzip` (ライブラリ API 経路) の 2 ケースを追加する
  - @voluntas
- [ADD] `examples/http11_server` に curl ベースの integration test を追加する
  - `tests/http_basic.rs` (GET / HEAD / POST / 404 の 7 ケース) を追加する
  - `tests/http_compression.rs` (gzip / br / zstd の Content-Encoding と優先順位の 9 ケース) を追加する
  - `tests/http_keep_alive.rs` (Keep-Alive と Connection: close の 3 ケース) を追加する
  - `tests/https_tls.rs` (rcgen で生成した自己署名証明書による HTTPS の 4 ケース) を追加する
  - `tests/helpers/mod.rs` にサーバー起動・kill ガード・curl 実行・証明書生成の共通ヘルパーを新設する
  - dev-dependencies に `rcgen` / `tempfile` / `tokio` (process feature) を追加する
  - @voluntas
- [ADD] `examples/http11_client` に testcontainers ベースの integration test を追加する
  - `tests/nginx_basic.rs` (GET 200 / 404 / HEAD / Server ヘッダー / HTTP/1.1 の 5 ケース) を追加する
  - `tests/nginx_streaming.rs` (gzip 由来の chunked 受信 / 1 MiB body / `Connection: close` 終端の 3 ケース) を追加する
  - `tests/helpers/mod.rs` に Docker 検出と nginx (`nginx:1.27-alpine`) 起動の共通ヘルパーを新設する
  - `src/lib.rs` を新設して `parse_url` / `http_request` / `https_request` / `decompressor` を pub にし、`src/main.rs` を CLI フロントエンドに整理する (`http_request` / `https_request` のシグネチャに `request_method: &str` を追加し、HEAD / CONNECT 経路で `BodyKind::None` / `Tunnel` を正しく扱う)
  - dev-dependencies に `testcontainers` (aws-lc-rs feature) と `tokio` を追加する
  - @voluntas
- [FIX] `examples/http11_server` / `examples/http11_server_io_uring` の死にコードを削除する
  - `compressor.rs` の `GzipCompressor` / `BrotliCompressor` / `ZstdCompressor` 構造体・`impl Default` / `impl Compressor` トレイト実装 (各約 200 行) を両 example から削除する。両 main.rs は自由関数 `compress_body` / `select_encoding` / `encoding_header` のみを使用しており、struct 群は呼び出し皆無の `#[allow(dead_code)]` 隠蔽コードだった
  - 特に `BrotliCompressor::compress` は単にバッファに溜めるだけで `finish()` で一括圧縮する偽ストリーミング実装で、お手本として残すのは積極的に有害だった
  - `examples/http11_server/src/main.rs::StreamingState::reset()` (`#[allow(dead_code)]` 付き、呼び出し皆無) を削除する
  - `examples/http11_server_io_uring/src/main.rs::DEFAULT_KEEP_ALIVE_TIMEOUT` (未使用 const + 「TODO: io_uring でタイムアウト処理を実装する際に使用」コメント) を削除する。タイムアウト処理を実装する際は新規に const を定義する
  - @voluntas
- [FIX] テストメッセージとコードコメントを日本語化し AGENTS.md 規約に準拠させる
  - `pbt/tests/` および `tests/` の英語 `prop_assert!` / `assert!` メッセージを日本語に統一する
  - `examples/http11_client/tests/` および `examples/http11_server/tests/` の `.expect(...)` / `panic!(...)` / `assert!(...)` 英語メッセージを日本語化する
  - `src/encoder.rs` / `src/decoder/response.rs` / `src/decoder/request.rs` / `examples/http11_client/src/main.rs` の英語コードコメントを日本語化または削除する
  - `pbt/tests/prop_request.rs` / `prop_content_type.rs` / `prop_accept.rs` の廃止 RFC 参照 (`RFC 7230`) を RFC 9110 Section 5.6.2 に更新する
  - 機能・ログメッセージ・エラーメッセージは変更しない
  - @voluntas
- [FIX] `HttpHead::is_keep_alive` の doc 内の誤った RFC 節番号 (Section 9.1 → Section 9.3 / Section 7.6.1) を修正する
  - @voluntas

## 2026.3.0

**リリース日**: 2026-05-06

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
- [CHANGE] `BodyProgress` を `Advanced` / `NeedData` / `Complete` の 3 値に細分化し、追加データが必要な状態を戻り値だけで判定できるようにする
  - 内部で利用していた非公開 `available_body_len()` を撤去し、`decode()` を `peek_body()` ベースに統一する
  - `src/decoder/mod.rs` のストリーミング API doc サンプルと `examples/http11_client` / `examples/http11_server` / `examples/http11_reverse_proxy` の `remaining_before` 比較ハックを 3 値パターンマッチに書き換える
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
