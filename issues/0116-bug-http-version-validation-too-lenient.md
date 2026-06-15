# HTTP-version の構文が RFC 9112 より緩い

- Priority: High
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-http-version-validation-too-lenient
- Polished: 2026-06-16

## 目的

HTTP-version の検証を RFC 9112 Section 2.3 に厳密に準拠させ、`"HTTP" "/" DIGIT "." DIGIT` 以外の形式を `Request` / `Response` / `RequestHead` / `ResponseHead` の HTTP-version として拒否する。同時に、現状 `with_version` 経由で構築可能だった RTSP の構築経路を **HTTP 専用 API から分離して** 別 API として整備する。

### スコープ整理 (AGENTS.md 整合性のための重要事項)

AGENTS.md L47 では「RTSP/1.0 や RTSP/2.0 も利用できること」と明記されている。本 issue を機械的にマージすると RTSP サポートが完全に消失し AGENTS.md の方針と矛盾する。これを回避するため本 issue は以下の **3 issue 群** として進める:

1. **本 issue (0116)**: `is_valid_http_version` を新設し、`Request` / `Response` / `RequestHead` / `ResponseHead` の `with_version` および encoder / decoder の HTTP-version 検証を `is_valid_http_version` に置き換える破壊的変更
2. **前提として並行起票する別 issue (0116a)**: RTSP/1.0 / RTSP/2.0 を構築・送受信するための専用 API (`Request::with_rtsp_version` 等、または別型 `RtspRequest` 等) の設計と実装。本 issue 0116 と同時に開発・同時にマージする (依存順としては 0116a → 0116 の順、または同一 PR にまとめる)
3. **AGENTS.md 改訂** (0116 と同一 PR で実施): AGENTS.md L47 を「RTSP/1.0 や RTSP/2.0 も利用できる (HTTP-version とは別 API 経由)」のような表現に更新

このスコープ整理を **本 issue 完了の前提条件** として明記する。本 issue だけが先行マージされて RTSP サポート消失期間が生じることを防ぐ。

別 issue 0116a の起票は `create-issue` スキル経由で本 issue polish 完了後に行う。

## 優先度根拠

High とする。RFC 9112 では `HTTP-version = HTTP-name "/" DIGIT "." DIGIT`、`HTTP-name = %x48.54.54.50 ; HTTP` と明記されている。現状の `is_valid_protocol_version` は `token "/" 1*DIGIT "." 1*DIGIT` を許容しており、`"http/1.1"` (小文字)、`"HTTP/11.1"` (major 多桁)、`"HTTP/1.11"` (minor 多桁)、`"HTTP/1.01"` (leading zero)、`"FOO/1.0"` 等を HTTP バージョンとして受理してしまう。これにより送信側で RFC 違反のバージョン文字列を生成できる経路が残っている。

## 現状

- `src/validate.rs:78` の `is_valid_protocol_version` は `token "/" 1*DIGIT "." 1*DIGIT` を許容している
- `src/encoder.rs:19` の `is_valid_version_for_encode` は VCHAR のみをチェックするため、HTTP-version として不適切な値も通過する
- `src/request.rs:112` の `Request::with_version` / `src/response.rs:144` の `Response::with_version` / `src/decoder/head.rs:195` の `RequestHead::with_version` / `src/decoder/head.rs:385` の `ResponseHead::with_version` / `src/decoder/request.rs:393` / `src/decoder/response.rs:464` は上記 `is_valid_protocol_version` を使用しており、HTTP-version として緩い
- `src/request.rs:181` / `src/response.rs:208` / `src/decoder/head.rs:325` / `src/decoder/head.rs:526` 等の `debug_assert!(is_valid_protocol_version(...))` も同様に緩い
- `src/lib.rs:9` のクレートドキュメントは「HTTP/1.1, RTSP/1.0, RTSP/2.0 等に対応」を謳う
- `src/uri.rs` には `Scheme::RTSP` 定数が存在する

## 設計方針

1. HTTP-version 用の検証では `"HTTP"` の大文字小文字区別の完全一致 (`HTTP-name = %x48.54.54.50` をリテラル比較で実装) と major / minor 各 1 桁 (DIGIT、0-9 のみ) を要求する。`HTTP/0.9` / `HTTP/1.0` / `HTTP/1.1` / `HTTP/2.0` / `HTTP/9.9` 等は受理される。`"http/1.1"`、`"HTTP/11.1"`、`"HTTP/1.11"`、`"HTTP/1.01"` (leading zero)、HTTP-name 以外 (例: `"FOO/1.0"`) は拒否される。

2. RTSP-version 等の他プロトコル用検証は既存の `is_valid_protocol_version` (`token "/" 1*DIGIT "." 1*DIGIT`) を **汎用プロトコルバージョン検証** として残す。HTTP 検証と混在させない。本 issue で `is_valid_protocol_version` 関数の signature・意味は変更しない。

3. HTTP 専用 API である `Request::with_version` / `Response::with_version` / `RequestHead::with_version` / `ResponseHead::with_version`、エンコーダー (`is_valid_version_for_encode` 廃止、`is_valid_http_version` に統一)、デコーダー (`RequestDecoder` / `ResponseDecoder` の start-line パース時のバージョン検証) は `is_valid_http_version` を使用する。これにより RTSP 等を HTTP-version として誤って構築・転送する経路を塞ぐ。

4. 上記 `with_version` 等で `debug_assert!(is_valid_protocol_version(...))` を呼んでいる箇所 (`request.rs:181`、`response.rs:208`、`decoder/head.rs:325`、`decoder/head.rs:526`) も `debug_assert!(is_valid_http_version(...))` に置き換える。

5. `is_valid_version_for_encode` (`src/encoder.rs:19`) は **削除** する。`is_valid_http_version` で代替可能であり、二重定義は混乱の元。

6. RTSP 構築・送受信は別 issue 0116a で導入される専用 API に委ねる。本 issue マージ時点で 0116a が同時マージされる前提なので、RTSP サポート消失期間は発生しない。0116a で導入される API のシグネチャ案 (本 issue 内では確定せず、0116a 側で議論する):
   - 案 A: `Request::with_rtsp_version(version: &str, ...)` のような専用 setter
   - 案 B: 別型 `RtspRequest` / `RtspResponse` の導入
   - 案 C: `with_version` の generic 化 (内部 enum で HTTP / RTSP を分岐)

   いずれの案でも本 issue の `is_valid_http_version` への置き換えは独立に実装可能。

7. AGENTS.md L47「RTSP/1.0 や RTSP/2.0 も利用できること」を「RTSP/1.0 や RTSP/2.0 も利用できる (HTTP-version とは別 API 経由)」に改訂し、本 issue 完了時に方針との整合性を保つ。改訂は本 issue PR と同一 PR で実施する。

8. `src/lib.rs:9` のクレートドキュメント「HTTP/1.1, RTSP/1.0, RTSP/2.0 等に対応」も、0116a で導入される RTSP 専用 API の説明を追記して整合性を保つ (本 issue PR と同一 PR で実施)。

## 完了条件

- `src/validate.rs` に `is_valid_http_version` 関数 (RFC 9112 Section 2.3 準拠) が追加されること。シグネチャ: `pub(crate) fn is_valid_http_version(version: &str) -> bool`。
- 以下が HTTP-version として **拒否** されること:
  - `"http/1.1"` (小文字)
  - `"HTTP/11.1"` (major 多桁)
  - `"HTTP/1.11"` (minor 多桁)
  - `"HTTP/1.01"` (minor の leading zero)
  - `"HTTP/01.1"` (major の leading zero)
  - `"FOO/1.0"` 等、HTTP-name 以外
  - `"RTSP/1.0"` / `"RTSP/2.0"` (HTTP-version としては拒否。RTSP 用 API では受理される、別 issue 0116a)
- 以下が HTTP-version として **受理** されること:
  - `"HTTP/1.1"`、`"HTTP/1.0"`、`"HTTP/2.0"`、`"HTTP/0.9"`、`"HTTP/9.9"`、`"HTTP/0.0"` (DIGIT は 0-9 範囲)
- エンコーダー (`src/encoder.rs:19`) の `is_valid_version_for_encode` を **削除** し、呼び出し箇所を `is_valid_http_version` に置き換えること (二重定義の解消)。
- デコーダー (`src/decoder/request.rs:393`、`src/decoder/response.rs:464`) の start-line パース時のバージョン検証を `is_valid_http_version` に置き換えること。RTSP メッセージのデコードは本 crate の `RequestDecoder` / `ResponseDecoder` では拒否される (RTSP 用デコーダーは 0116a で別途整備)。
- `with_version` 系の `debug_assert!(is_valid_protocol_version(...))` (`request.rs:181`、`response.rs:208`、`decoder/head.rs:325`、`decoder/head.rs:526`) を `is_valid_http_version` に置き換えること。
- RTSP サポートは別 issue 0116a で別 API として整備され、本 issue PR と同一 PR (または同時マージ) で完了すること (RTSP サポート消失期間ゼロ)。
- AGENTS.md L47 を「RTSP/1.0 や RTSP/2.0 も利用できる (HTTP-version とは別 API 経由)」のように更新すること (本 issue PR と同一 PR で実施)。
- `src/lib.rs:9` のクレートドキュメントを更新し、RTSP 用 API の存在を明示すること (本 issue PR と同一 PR で実施)。
- 以下のテストを追加・更新すること:
  - `src/validate.rs` インラインテスト: `is_valid_http_version` の受理 / 拒否パターン全列挙
  - `tests/test_request.rs`: `Request::with_version` の拒否・受理ケース。既存の RTSP 受理テスト (`test_request_with_version_accepts_rtsp_versions`) と `is_keep_alive` の RTSP/1.1 ケースは **削除** する (RTSP は 0116a で別 API テスト) 。
  - `tests/test_response.rs`: `Response::with_version` の拒否・受理ケース、既存の RTSP 関連テストを削除
  - `tests/test_decoder/head.rs` および `tests/test_decoder/decode_body.rs`: デコーダー経由の HTTP-version 拒否・受理ケース
  - `tests/test_decoder/decode_body.rs:284-296,354-365` の RTSP/FOO Transfer-Encoding 拒否テストは、`is_valid_http_version` で start-line 段階で reject されるため、テスト名と意図を変更する。**HTTP/0.9 / HTTP/2.0 / HTTP/3.0** のような未対応・将来 HTTP バージョンで `assert_request_te_rejected` を回す形に変える (HRS = CWE-444 防御の検証目的は維持)。RTSP/FOO の reject は別 (start-line 段階の RFC 9112 §2.3 違反として) で担保し、テスト名を `test_te_rejected_on_unsupported_http_version` 等に変更する。
  - `pbt/tests/prop_response.rs`、`pbt/tests/prop_decoder/request.rs`、`pbt/tests/prop_decoder/response/status_line.rs`: `Response::with_version` やデコーダー入力に RTSP バージョンを生成している箇所を HTTP バージョンに絞る (HRS PBT は `is_valid_http_version` 通過後の HTTP/0.9 / 2.0 / 3.0 で網羅する)
  - `tests/test_request.rs:585-619` (`test_request_is_keep_alive_rtsp_or_foo_11_not_keep_alive_by_default`) は `with_version("RTSP/1.1", ...)` 自体が Err になるためテスト存在意義が消える。**完全に削除** する。`is_keep_alive` 内部の RTSP/1.1 ガード (`head.rs:108-111`) は HTTP-version 検証通過後の値しか見ないため到達不能になる。to-do として `head.rs:108-111` のガードコードも **削除** し、`is_keep_alive` を HTTP-only と明示する rustdoc に更新する。
- `CHANGES.md` の `## develop` セクションに `[CHANGE]` として以下を記載すること:
  - `Request::with_version` / `Response::with_version` / `RequestHead::with_version` / `ResponseHead::with_version` を HTTP-version 専用に厳格化し、RTSP 等の他プロトコル文字列を拒否するようになったこと
  - `RequestDecoder` / `ResponseDecoder` が start-line パース時に HTTP-version 厳格検証を行い、RTSP メッセージは受理されなくなったこと (受信挙動の変更)
  - RTSP サポートは別 API として整備される (0116a 参照)
  - エンコーダーの `is_valid_version_for_encode` を削除し `is_valid_http_version` に統一

## 解決方法

設計方針に沿って次の順序で実装する。

1. 別 issue 0116a を `create-issue` スキル経由で起票し、RTSP 用 API の設計と実装を本 issue と並行で進める。
2. `src/validate.rs` に `is_valid_http_version` 関数を追加する (RFC 9112 Section 2.3 準拠、`HTTP-name = "HTTP"` リテラル + `/` + DIGIT + `.` + DIGIT 厳密判定)。
3. `is_valid_protocol_version` は変更せず、用途を rustdoc コメントで「RTSP 等の汎用プロトコルバージョン検証」と明確化する。
4. `src/request.rs`、`src/response.rs`、`src/decoder/head.rs`、`src/decoder/request.rs`、`src/decoder/response.rs`、`src/encoder.rs` の HTTP-version 検証 (`is_valid_protocol_version` / `is_valid_version_for_encode`) を `is_valid_http_version` に置き換える。`debug_assert!` も同様に置き換える。
5. `src/encoder.rs:19` の `is_valid_version_for_encode` 関数を削除する。
6. `src/decoder/head.rs:108-111` の `is_keep_alive` の RTSP/1.1 ガードを削除し、HTTP-only と明示する rustdoc に更新する。
7. AGENTS.md L47 を更新する (本 issue PR と同一 PR で実施)。
8. `src/lib.rs:9` のクレートドキュメントを更新する (本 issue PR と同一 PR で実施)。
9. 上記完了条件のテストを追加・更新・削除する。
10. `CHANGES.md` の `## develop` セクションに `[CHANGE]` エントリを追加する。

## 参考: RFC 文面

- RFC 9112 Section 2.3 HTTP Version
  > HTTP's version number consists of two decimal digits separated by a "." (period or decimal point). The first digit (major version) indicates the messaging syntax, whereas the second digit (minor version) indicates the highest minor version within that major version to which the sender is conformant (able to understand for future communication).
  > HTTP-version = HTTP-name "/" DIGIT "." DIGIT
  > HTTP-name = %s"HTTP"
- RFC 9112 Section 2.3 (collected ABNF より補足)
  > HTTP-name = %x48.54.54.50 ; HTTP
- RFC 9110 Section 15.6.6 505 HTTP Version Not Supported
  > The 505 (HTTP Version Not Supported) status code indicates that the server does not support, or refuses to support, the major version of HTTP that was used in the request message.
