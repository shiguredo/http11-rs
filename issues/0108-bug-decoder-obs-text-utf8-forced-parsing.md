# デコーダーが reason-phrase / field-value / trailer の obs-text を UTF-8 強制解釈で拒否する

- Priority: High
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-decoder-obs-text-utf8-forced-parsing
- Polished: 2026-06-13

## 目的

RFC 9110 Section 5.5 / RFC 9110 Section 5.6.4 / RFC 9112 Section 4 / RFC 9112 Section 5.1 / RFC 9112 Section 7.1.2 において、reason-phrase / field-value / quoted-string / chunked trailer の field-line は obs-text（`%x80-FF`）を許容する。現状のデコーダーはこれらの行を `String::from_utf8` で UTF-8 として強制解釈しており、`0x80-0xFF` の単一バイトを含む有効な HTTP/1.1 メッセージを `invalid UTF-8` として拒否している。この問題を修正し、obs-text を Unicode scalar 拡張解釈（`U+0080..=U+00FF`）で受理・保持できるようにする。

request-line 自体の ABNF（RFC 9112 Section 3）では obs-text は許容しない。本 issue では request-line もバイト列ベースに改修するが、これは RFC 9112 Section 2.2 の「HTTP メッセージはオクテット列として解析すべき（MUST）」という解析方法の要件に従うためであり、method / request-target / HTTP-version の ASCII 限定性は維持する。

## 優先度根拠

High とする。RFC 9112 Section 2.2 は HTTP メッセージを「US-ASCII のスーパーセットとなるエンコーディングのオクテット列として解析しなければならない（MUST）」と規定している。`String::from_utf8` は UTF-8 として valid なバイト列のみを受け入れるため、`0x80-0xFF` の単一 obs-text バイトを含む有効な HTTP/1.1 メッセージを拒否してしまい、上記 MUST 要件に違反している。また RFC 9110 Section 5.5 は obs-text を opaque data として扱うべき（SHOULD）と規定しており、`AGENTS.md` でもプロジェクト方針として「obs-text（0x80-FF）は opaque data として保持すること」と明記している。デコーダーが UTF-8 強制解釈によって obs-text を拒否している現状は、この方針を満たしていない。

## 現状

`src/decoder/request.rs` / `src/decoder/response.rs` / `src/decoder/body.rs` の以下の箇所で、行全体を `String::from_utf8` または同等の UTF-8 変換で解釈している（現行の行番号は `develop` ブランチ 2026-06-13 時点）。

- `RequestDecoder::decode_headers` の request-line パース（`src/decoder/request.rs:345`）
- `ResponseDecoder::decode_headers` の status-line パース（`src/decoder/response.rs:442`）
- `RequestDecoder::decode_headers` の header-line パース（`src/decoder/request.rs:541`）
- `ResponseDecoder::decode_headers` の header-line パース（`src/decoder/response.rs:582`）
- `BodyDecoder::process_trailers` の trailer-line パース（`src/decoder/body.rs:508`）

これにより、以下のような有効な HTTP/1.1 メッセージが `invalid UTF-8` として拒否される。

- `HTTP/1.1 200 OK\xFF\r\n`（reason-phrase 末尾に obs-text）
- `X-Custom: \x80\r\n`（field-value に obs-text）
- chunked の trailer-section で `Trailer-Field: \xFF\r\n`

なお `src/decoder/body.rs` の `process_chunked_size`（`src/decoder/body.rs:363`） における chunk-size 行は既にバイト列ベースで解析しており、ここは対象外である。

## 設計方針

1. request-line / status-line / header-line / trailer-line のパースを、行全体の UTF-8 強制解釈をやめてバイト列ベースに改修する。request-line / status-line 内の SP 位置はバイト列スキャンで求め、reason-phrase / field-value 部分を `bytes_to_opaque_string` に渡す。

   request-line の擬似コード:
   ```text
   let first_sp = line_bytes.iter().position(|&b| b == b' ').ok_or(...)?;
   let second_sp = line_bytes[first_sp+1..].iter().position(|&b| b == b' ')
       .map(|p| first_sp + 1 + p).ok_or(...)?;
   let method = &line_bytes[..first_sp];
   let target = &line_bytes[first_sp+1..second_sp];
   let version = &line_bytes[second_sp+1..];
   // method は is_valid_token、version は is_valid_protocol_version、target は is_valid_request_target +
   // non-ASCII 追加拒否で検証。request-line 全体では obs-text（non-ASCII / 空白）は許容しない。
   ```

   status-line の擬似コード:
   ```text
   let first_sp = line_bytes.iter().position(|&b| b == b' ').ok_or(...)?;
   let second_sp = line_bytes[first_sp+1..].iter().position(|&b| b == b' ')
       .map(|p| first_sp + 1 + p).ok_or(...)?;
   let version = &line_bytes[..first_sp];
   let status_bytes = trim_ows_bytes(&line_bytes[first_sp+1..second_sp]);
   let reason = &line_bytes[second_sp+1..];
   ```

   status-code は ASCII 数字 3 桁（`100..=599`）としてバイト列で検証する。RFC 9112 Section 4 の ABNF `reason-phrase = 1*(...)` では reason-phrase は 1 文字以上だが、`status-line = HTTP-version SP status-code SP [ reason-phrase ]` でも reason-phrase 自体は optional である。実装上の寛容さとして、status-code 後の SP に続く値が空の場合は reason-phrase absent として空文字列を許容する。

   `RequestDecoder` / `ResponseDecoder` が保持する `start_line: Option<String>` フィールドは削除する。request-line は method / request-target / HTTP-version が ASCII なので元々 `String` に変換できたが、start-line 全体を保持する必要はなく、パース時に個別に必要な値を一時保持すれば十分である。response の status-line は reason-phrase に obs-text を含む可能性があるため、line 全体を `String` に保持すると複雑になる。`ResponseDecoder` では `version` / `status_code` / `reason_phrase` を個別に一時保持し、`ResponseHead` 構築時に組み立てる。request.rs / response.rs で既存の `start_line` を参照している箇所（`Host` 検証、`CONNECT` 判定、`determine_body_kind` の version 取得等）は、個別に保持した値を参照するよう変更する。

2. obs-text オクテット（`0x80-0xFF`）は失わず、Unicode scalar 拡張解釈（`U+0080..=U+00FF`）で保持する。`src/decoder/utils.rs` を新設し、`bytes_to_opaque_string(bytes: &[u8]) -> Result<String, Error>` をそこに配置する。

   ```text
   1. 入力バイト列を先頭からインデックス i で走査する。
   2. i 位置の先頭バイトを見て、期待される UTF-8 シーケンス長 n（1, 2, 3, 4）を決定する。
      - 0x00-0x7F: n = 1
      - 0xC2-0xDF: n = 2
      - 0xE0-0xEF: n = 3
      - 0xF0-0xF4: n = 4
      - 0x80-0xBF / 0xC0-0xC1 / 0xF5-0xFF: invalid な先頭バイトとして n = 1（1 バイトずつ obs-text として処理）
   3. n > 1 の場合、後続 n-1 バイトが continuation バイト（0x80-0xBF）かつシーケンス全体が RFC 3629 に従った valid UTF-8（U+0000..=U+10FFFF、surrogate 除く、overlong エンコーディングを含まない）を表すかを検証する。
      - valid であれば対応する Unicode scalar の char を push し、i を n 進める。
      - invalid / incomplete / surrogate / overlong であれば、i 位置の 1 バイトのみを obs-text バイトとして処理する。
   4. 1 バイト処理時:
      - 0x00 / 0x0D / 0x0A は Err(Error::InvalidData(...)) を返す。
      - 0x01-0x09 / 0x0B-0x0C / 0x0E-0x7F はそのまま char として push。
      - 0x80-0xFF は char::from_u32(b as u32).unwrap() で U+0080..=U+00FF の char として push。
   5. 全バイトを消費するまで繰り返す。
   ```

   具体例:
   - `b"\x80"` → `"\u{0080}"`
   - `b"\xFF"` → `"\u{00FF}"`
   - `b"\xC4\x80"` → `"\u{0100}"`（valid UTF-8）
   - `b"\xC3\x28"` → `"\u{00C3}("`（invalid sequence: 先頭バイトを obs-text、次を ASCII）
   - `b"\xC3"` → `"\u{00C3}"`（incomplete）
   - `b"\xED\xA0\x80"` → `"\u{00ED}\u{00A0}\u{0080}"`（surrogate を表す UTF-8 バイト列を 1 バイトずつ処理）

   `bytes_to_opaque_string` は「UTF-8 不変条件を保ちながら HTTP メッセージ行のバイト列を Rust の `String` に変換する」decoder 専用関数である。本関数は原則として HTTP バイト列 → `String` の変換のみを行い、VCHAR/SP/HTAB/obs-text 以外の文字（特に NUL/CR/LF）の文法検証は `_opaque` 版 validator に委ねる。ただし NUL/CR/LF はメッセージフレーミングを破壊するため、安全のため本関数内で事前に reject する。`_opaque` validator では `0x7F`（DEL）も `VCHAR / obs-text` のいずれでもないため reject するが、本関数では変換のみを行うので `0x7F` もそのまま push する。surrogate を表す UTF-8 バイト列は 1 バイトずつ obs-text として処理し、`String` 内に surrogate は含まれないことを保証する（`AGENTS.md`「surrogate 除く」準拠）。

   `AGENTS.md` の `char_indices()` 規定は `&str` 入力を想定しているが、本関数は decoder 上流の `&[u8]` を扱うため byte インデックスベースの走査を用いる。これは `char_indices()` と同等に UTF-8 不変条件を保つ。

3. `RequestHead` / `ResponseHead` 内部の文字列表現は `String` のまま維持する。`AGENTS.md` の方針に従い、`String` 内の各 `char` は「HTTP 上の 1 バイト obs-text として `U+0080..=U+00FF` にマップされたもの、または有効な UTF-8 マルチバイトシーケンス由来の Unicode scalar（`U+0000..=U+10FFFF`、surrogate 除く）のいずれか」という不変条件を満たす。

   ただしこの表現は元の HTTP バイト列を完全に復元できない。例えば HTTP 上の 1 バイト `0x80` と、valid UTF-8 シーケンス `0xC2 0x80`（U+0080）はどちらも `String` 内の 1 文字 `U+0080` になってしまい区別不能である。したがって `String::len()` は HTTP バイト長と一致せず、Content-Length 計算・ヘッダー行長制限・透明転送には注意が必要である。

4. `src/validate.rs` の `is_valid_field_value` / `is_valid_reason_phrase` は builder 用に既存の byte 判定のまま維持する。decoder 専用に以下を新設する。
   - `is_valid_field_value_opaque(value: &str) -> bool`: `value.chars().all(|c| matches!(c, '\t' | ' ' | '!'..='~' | '\u{0080}'..='\u{10FFFF}'))`
   - `is_valid_reason_phrase_opaque(phrase: &str) -> bool`: `phrase.chars().all(|c| matches!(c, '\t' | ' ' | '!'..='~' | '\u{0080}'..='\u{10FFFF}'))`

   `\u{0080}..=\u{10FFFF}` は surrogate 範囲（`U+D800..=U+DFFF`）を含むが、`&str` には surrogate は含まれず、`bytes_to_opaque_string` でも生成されないため、実行時には問題ない。空の reason-phrase は absent 扱いとして許容するが、これは validator ではなく呼び出し側で制御する。

5. `parse_header_line` を `&[u8]` 対応にする。`find_line`（`src/decoder/body.rs:733`） は既に `&[u8]` 対応なので変更不要。新しい signature と内部フローは以下の通り。`trim_ows_bytes` は `pub(crate) fn trim_ows_bytes(bytes: &[u8]) -> &[u8]` とし、`trim_ows`（`src/validate.rs:396`） と同様に先頭・末尾の SP/HTAB のみをスキップする byte ベース版とする。

   ```text
   pub(crate) fn parse_header_line(line: &[u8]) -> Result<(String, String), Error> {
       // 1. 空行チェック。空なら invalid header line: empty
       // 2. 行頭 obs-fold 拒否 (line[0] == b' ' || line[0] == b'\t')（RFC 9112 Section 5.2 準拠）
       // 3. 行内の NUL / CR / LF 拒否
       // 4. 最初の ':' バイトで name / value を分割。':' がなければ reject
       // 5. name バイト列を is_valid_header_name（内部は byte ベース）で検証し、
       //    ASCII token なので安全に str に変換して to_string() する
       // 6. value 部分から byte ベースで前後 OWS を除去（trim_ows_bytes 新設）
       // 7. 除去後の value バイト列を bytes_to_opaque_string で String 化
       // 8. is_valid_field_value_opaque で検証
       // 9. Ok((name.to_string(), value_string))
   }
   ```

6. セマンティックヘッダー値（`Content-Length` / `Transfer-Encoding` / `Host` / `Connection` / `Trailer` 等）に obs-text が混入した場合は、HTTP Request Smuggling / Response Splitting 対策として構文エラーとする。これらの値は `parse_header_line` 後の `String` として個別のパーサー（`parse_content_length_value` / `parse_transfer_encoding_*` / `collect_declared_trailers` 等）に渡されるが、これらのパーサーは ASCII 数字・token・OWS のみを受理するため、obs-text が含まれれば自然にエラーとなる。

7. `src/auth.rs` / `src/content_disposition.rs` の quoted-string パーサーは `&str` 版のまま変更しない。decoder 側で field-value 全体を `bytes_to_opaque_string` で valid UTF-8 `String` に変換してから `Authorization::parse` / `ContentDisposition::parse` に `&str` として渡すことで、quoted-string パーサーは RFC 9112 Section 2.2 に従って抽出済みの protocol element 内で動作するため、`&str` 版のままで obs-text は `is_qdtext_char` / `is_quoted_pair_char`（issue 0059 の成果）に任せて受理される。`bytes_to_opaque_string` の出力は valid UTF-8 `String` であるため、0059 issue の前提（decoder 上流で valid UTF-8 が保証される）は維持される。

8. `RequestHead::from_validated_parts`（`src/decoder/head.rs:314`） / `ResponseHead::from_validated_parts`（`src/decoder/head.rs:519`） の `debug_assert!` を以下のように変更する。併せて rustdoc も更新する。

   `RequestHead::from_validated_parts`:
   - `is_valid_request_target(&uri)`: 現状維持（`is_valid_request_target` は obs-text を許容するまま）。decoder 側で request-target の non-ASCII 追加拒否を行うことに言及。
   - `is_valid_protocol_version`: 変更なし。
   - header values を `is_valid_field_value_opaque` に変更。

   `ResponseHead::from_validated_parts`:
   - `is_valid_protocol_version`: 変更なし。
   - `is_valid_status_code`: 変更なし。
   - reason_phrase を `(empty || is_valid_reason_phrase_opaque(...))` に変更。空は absent として許容。
   - header values を `is_valid_field_value_opaque` に変更。

9. 送信側 builder / encoder については本 issue では扱わない。`String` 内の `U+0080..=U+00FF` を `String::as_bytes()` すると UTF-8 化されて元の obs-text 1 バイトと異なるため、`examples/http11_reverse_proxy` のような透明転送が必要な経路では別途対応が必要。完全な透明転送を実現するための内部表現変更または encoder 側変換方式の検討は follow-up issue として作成する。

10. RTSP/1.0 や RTSP/2.0 でも同様のメッセージ構文を処理するため、本変更は HTTP/1.1 に限らず RTSP の start-line / header-line でも適用される。

## 完了条件

- obs-text（`0x80-0xFF`）を含む reason-phrase / field-value / chunked trailer を持つ有効な HTTP/1.1 メッセージが `RequestDecoder` / `ResponseDecoder` で受理されるようになること。
- 以下の具体例を含むデコードテストを `tests/test_decoder/head.rs` / `tests/test_decoder/body.rs` に追加すること。
  - reason-phrase に `0x80` / `0xFF` を含むレスポンス
  - field-value に `0x80` / `0xFF` を含むリクエストとレスポンス
  - chunked trailer の field-value に `0x80` / `0xFF` を含むメッセージ
  - 有効な UTF-8 マルチバイト（例: `U+0100`）と obs-text の混在
  - incomplete UTF-8 シーケンス（例: `0xC3` だけ、`0xF0 0x90` だけ）を含む field-value / reason-phrase
- NUL / CR / LF を含む field-value / reason-phrase / chunked trailer は引き続き拒否されること（RFC 9110 Section 5.5 / RFC 9112 Section 4 準拠）。
- method（RFC 9112 Section 3.1 / Appendix A `token`）/ request-target（RFC 3986 Section 2.1-2.3、RFC 9112 Section 3.2）/ HTTP-version（RFC 9112 Section 2.3）/ header-name（RFC 9110 Section 5.1 `token`）に `0x80-0xFF` を含むメッセージは引き続き拒否されること。
- 既存の UTF-8 only メッセージのデコード挙動が変わらないこと。
- 以下の既存 PBT を更新すること。
  - `pbt/tests/prop_decoder/request.rs`
    - `prop_invalid_utf8_request_line_error`: request-target / method / HTTP-version に obs-text を含むため、**引き続き拒否**されるケースに名称変更・整理する。例: `prop_obs_text_in_request_line_still_rejected`。
    - `prop_invalid_utf8_header_error`: ヘッダー値の obs-text を **受理** する方向に反転する。`decode_headers().is_ok()` を確認し、`head.get_header("X-Obs")` が `U+0080` / `U+00FF` 等を含むことを `assert_eq!` で検証する。
  - `pbt/tests/prop_decoder/response/status_line.rs`
    - `prop_invalid_utf8_status_line_error`: reason-phrase の obs-text を **受理** する方向に反転する。`decode_headers().is_ok()` を確認し、`head.reason_phrase()` が `U+0080` / `U+00FF` 等を含むことを検証する。
    - `prop_invalid_utf8_response_header_error`: ヘッダー値の obs-text を **受理** する方向に反転する。`head.get_header("X-Obs")` の内容を検証する。
  - 新規に「引き続き拒否」ケースを追加する: request-target / header-name / method / HTTP-version に obs-text を含むケース。入力として単一 obs-text バイト `0x80` / `0xFF`、有効な UTF-8 マルチバイト `0xC4 0x80`（U+0100）と obs-text の混在、incomplete UTF-8 シーケンス `0xC3` のみ、`0xF0 0x90` のみを含める。
- 以下の fuzz target を確認・必要に応じて修正すること。
  - `fuzz_decoder_request` / `fuzz_decoder_response`（`fuzz/fuzz_targets/fuzz_decoder_request.rs` / `fuzz_decoder_response.rs`）: obs-text を含む入力が新たに受理される経路を確認。fuzz target 内で固定の obs-text ヘッダーを含むバイト列（例: `b"GET / HTTP/1.1\r\nHost: localhost\r\nX-Obs: \x80\r\n\r\n"` および `b"HTTP/1.1 200 \xFF\r\n\r\n"`）を生成し、`decode_headers().is_ok()` を assert する追加シナリオを記載する。これは既存の任意入力によるパニック安全性検証に加えて、obs-text 受理経路が実行時に到達することを保証するための固定ケースである。
  - `fuzz_chunked_trailer`（`fuzz/fuzz_targets/fuzz_chunked_trailer.rs`）: `FuzzInput.trailers` の value 型を `Vec<u8>` に変更し、`Vec<(String, Vec<u8>)>` とする。obs-text 単一バイト（`0x80-0xFF`）を直接バイト列に含めるように修正。`normalize_trailers` 内の `is_valid_value` を byte ベースに変更し、`build_trailer_section` で `Vec<u8>` 値をそのままバイト列に書き込む。
- `src/validate.rs` の `is_valid_field_value`（`src/validate.rs:53`） / `is_valid_reason_phrase`（`src/validate.rs:127`） のコメントを「builder 用。入力は valid UTF-8 のみを想定しており、HTTP 上の 1 バイト obs-text（0x80-0xFF）は `String` 内では UTF-8 マルチバイト（例: U+0080 → 0xC2 0x80）として表現されるが、この関数は ABNF 準拠の `VCHAR / obs-text` を byte 値で判定するため、builder からの obs-text は実質的に受理できない」という意味に更新する。
- `src/validate.rs` の `is_valid_request_target`（`src/validate.rs:157`） の rustdoc / コメントを更新し、「構文上は obs-text を含まないが歴史的互換性のため許容している。decoder 側では RFC 9112 Section 3.2 / RFC 3986 に従い non-ASCII を追加拒否する」旨を明記する。
- `src/decoder/head.rs` の `RequestHead::from_validated_parts`（`src/decoder/head.rs:314`） / `ResponseHead::from_validated_parts`（`src/decoder/head.rs:519`） の rustdoc を更新し、`_opaque` 版 validator の使用と decoder 側での request-target 追加拒否を明記する。
- `src/decoder/request.rs` / `src/decoder/response.rs` / `src/decoder/body.rs` のモジュール先頭コメント（それぞれ先頭 11 行程度）を更新し、UTF-8 強制解析の非準拠を削除する。
- `README.md` の「既知の制限事項」（`README.md:434`）を更新する。obs-text は Unicode scalar 拡張解釈で受理・保持されるが、`String::as_bytes()` すると元の 1 バイトとは異なる UTF-8 表現となること、完全な透明転送が必要な場合は内部表現変更または encoder 側対応が必要である旨を追記する。
- 完全な透明転送を実現するための follow-up issue を作成する。
- `CHANGES.md` の `## develop` セクションに以下の `[FIX]` エントリを `### misc` の上に追加する。
  - `[FIX] decoder が reason-phrase / field-value / chunked trailer の obs-text（0x80-0xFF）を UTF-8 強制解釈で拒否していた問題を修正。obs-text は Unicode scalar 拡張解釈（U+0080..=U+00FF）で受理・保持するようにした。`

## 解決方法

設計方針に従い、以下を実装する。

- `src/decoder/request.rs` / `src/decoder/response.rs` / `src/decoder/body.rs` の `String::from_utf8` による行解析をバイト列ベースの解析に置き換える。`RequestDecoder` / `ResponseDecoder` の `start_line: Option<String>` フィールドを削除し、個別の一時フィールドに置き換える。
- `src/decoder/body.rs` の `parse_header_line` を `&[u8]` 対応に変更する。
- `src/decoder/utils.rs` を新設し、`bytes_to_opaque_string` / `trim_ows_bytes` を配置する。
- `src/decoder/mod.rs` に `mod utils;` を追加し、`request.rs` / `response.rs` / `body.rs` から `utils` モジュールを参照する。
- `src/validate.rs` に `is_valid_field_value_opaque` / `is_valid_reason_phrase_opaque` を新設する。
- `src/decoder/head.rs` の `RequestHead::from_validated_parts` / `ResponseHead::from_validated_parts` の `debug_assert!` と rustdoc を `_opaque` 版 validator を使うように変更する。
- `src/auth.rs` / `src/content_disposition.rs` は `&str` 版のまま変更せず、decoder 側で field-value 全体を変換してから渡す。
- 既存 PBT / fuzz target / 単体テストを更新する。
- `README.md` / モジュールコメント / `is_valid_field_value` / `is_valid_reason_phrase` のコメントを更新する。
- follow-up issue を作成する。

## 参考: RFC 文面

- RFC 9110 Section 5.1
  > field-name = token
- RFC 9110 Section 5.5
  > field-value = *field-content
  > field-content = field-vchar [ 1*( SP / HTAB / field-vchar ) field-vchar ]
  > field-vchar = VCHAR / obs-text
  > obs-text = %x80-FF
  > A field value does not include leading or trailing whitespace. When a specific version of HTTP allows such whitespace to appear in a message, a field parsing implementation MUST exclude such whitespace prior to evaluating the field value.
  > A recipient SHOULD treat other allowed octets in field content (i.e., obs-text) as opaque data.
  > Field values containing CR, LF, or NUL characters are invalid and dangerous, due to the varying ways that implementations might parse and interpret those characters; a recipient of CR, LF, or NUL within a field value MUST either reject the message or replace each of those characters with SP before further processing or forwarding of that message.
  > Field values containing other CTL characters are also invalid; however, recipients MAY retain such characters for the sake of robustness when they appear within a safe context (e.g., an application-specific quoted string that will not be processed by any downstream HTTP parser).
- RFC 9110 Section 5.6.2
  > token = 1*tchar
  > tchar = "!" / "#" / "$" / "%" / "&" / "'" / "*" / "+" / "-" / "." / "^" / "_" / "`" / "|" / "~" / DIGIT / ALPHA
- RFC 9110 Section 5.6.4
  > quoted-string = DQUOTE *( qdtext / quoted-pair ) DQUOTE
  > qdtext = HTAB / SP / %x21 / %x23-5B / %x5D-7E / obs-text
  > quoted-pair = "\" ( HTAB / SP / VCHAR / obs-text )
- RFC 9112 Section 2.2
  > A recipient MUST parse an HTTP message as a sequence of octets in an encoding that is a superset of US-ASCII [USASCII].
  > Parsing an HTTP message as a stream of Unicode characters, without regard for the specific encoding, creates security vulnerabilities due to the varying ways that string processing libraries handle invalid multibyte character sequences that contain the octet LF (%x0A).
  > String-based parsers can only be safely used within protocol elements after the element has been extracted from the message, such as within a header field line value after message parsing has delineated the individual field lines.
- RFC 9112 Section 2.3
  > HTTP-version = HTTP-name "/" DIGIT "." DIGIT
  > HTTP-name = %s"HTTP"
- RFC 9112 Section 3
  > A request-line begins with a method token, followed by a single space (SP), the request-target, and another single space (SP), and ends with the protocol version.
  > request-line = method SP request-target SP HTTP-version
  > method = token
- RFC 9112 Section 3.2
  > request-target = origin-form / absolute-form / authority-form / asterisk-form
  > No whitespace is allowed in the request-target.
- RFC 9112 Section 4
  > status-line = HTTP-version SP status-code SP [ reason-phrase ]
  > reason-phrase = 1*( HTAB / SP / VCHAR / obs-text )
- RFC 9112 Section 5.1
  > field-line = field-name ":" OWS field-value OWS
  > The field line value does not include that leading or trailing whitespace: OWS occurring before the first non-whitespace octet of the field line value, or after the last non-whitespace octet of the field line value, is excluded by parsers when extracting the field line value from a field line.
- RFC 9112 Section 5.2
  > A sender MUST NOT generate a message that includes line folding (i.e., that has any field line value that contains a match to the obs-fold rule) unless the message is intended for packaging within the "message/http" media type.
- RFC 9112 Section 7.1.2
  > trailer-section = *( field-line CRLF )
- RFC 3986 Section 2.1
  > pct-encoded = "%" HEXDIG HEXDIG
- RFC 3986 Section 2.2
  > reserved = gen-delims / sub-delims
  > gen-delims = ":" / "/" / "?" / "#" / "[" / "]" / "@"
  > sub-delims = "!" / "$" / "&" / "'" / "(" / ")" / "*" / "+" / "," / ";" / "="
- RFC 3986 Section 2.3
  > unreserved = ALPHA / DIGIT / "-" / "." / "_" / "~"
