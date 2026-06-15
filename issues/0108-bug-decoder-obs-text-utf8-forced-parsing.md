# デコーダーが reason-phrase / field-value / trailer の obs-text を UTF-8 強制解釈で拒否する

- Priority: High
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-decoder-obs-text-utf8-forced-parsing
- Polished: 2026-06-15

## 目的

RFC 9110 Section 5.5 / RFC 9110 Section 5.6.4 / RFC 9112 Section 4 / RFC 9112 Section 5.1 / RFC 9112 Section 7.1.2 において、reason-phrase / field-value / quoted-string / chunked trailer の field-line は obs-text (`%x80-FF`) を許容する。現状のデコーダーはこれらの行を `String::from_utf8` で UTF-8 として強制解釈しており、`0x80-0xFF` の単一バイトを含む有効な HTTP/1.1 メッセージを `invalid UTF-8` として拒否している。本 issue でこれを修正し、obs-text を保持したまま受理できるようにする。

request-line 自体の ABNF (RFC 9112 Section 3) では obs-text は許容しない。本 issue では request-line もバイト列ベースに改修するが、これは RFC 9112 Section 2.2 の「HTTP メッセージはオクテット列として解析しなければならない (MUST)」という解析方法の要件に従うためであり、method / request-target / HTTP-version の ASCII 限定性は維持する。

なお `String::len()` は HTTP バイト長と一致しなくなる (1 バイト obs-text を 1 char に詰めるため再エンコードすると 2 バイト UTF-8 になる)。本 issue の **スコープは受理経路 (decoder の MUST 違反解消) のみ**。proxy / encoder 経由の透明転送 (Content-Length 整合性・原バイト復元) は別 issue で扱う (本 issue 完了条件で follow-up issue 番号を確保する)。

## 優先度根拠

High とする。RFC 9112 Section 2.2 は HTTP メッセージを「US-ASCII のスーパーセットとなるエンコーディングのオクテット列として解析しなければならない (MUST)」と規定している。`String::from_utf8` は UTF-8 として valid なバイト列のみを受け入れるため、`0x80-0xFF` の単一 obs-text バイトを含む有効な HTTP/1.1 メッセージを拒否してしまい、上記 MUST 要件に違反している。また RFC 9110 Section 5.5 は obs-text を opaque data として扱うべき (SHOULD) と規定しており、`AGENTS.md` でもプロジェクト方針として「obs-text (0x80-FF) は opaque data として保持すること」と明記している。デコーダーが UTF-8 強制解釈によって obs-text を拒否している現状は、この方針を満たしていない。

## 現状

`src/decoder/request.rs` / `src/decoder/response.rs` / `src/decoder/body.rs` の以下の箇所で、行全体を `String::from_utf8` で解釈している (現行の行番号は `develop` ブランチ 2026-06-15 時点)。

- `RequestDecoder::decode_headers` の request-line パース (`src/decoder/request.rs:345`)
- `ResponseDecoder::decode_headers` の status-line パース (`src/decoder/response.rs:442`)
- `RequestDecoder::decode_headers` の header-line パース (`src/decoder/request.rs:541`)
- `ResponseDecoder::decode_headers` の header-line パース (`src/decoder/response.rs:583`)
- `BodyDecoder::process_trailers` の trailer-line パース (`src/decoder/body.rs:508`)

これにより、以下のような有効な HTTP/1.1 メッセージが `invalid UTF-8` として拒否される。

- `HTTP/1.1 200 OK\xFF\r\n` (reason-phrase 末尾に obs-text)
- `Field-Value: \x80\r\n` (field-value に obs-text)
- chunked の trailer-section で `Trailer-Field: \xFF\r\n`

なお `src/decoder/body.rs` の `process_chunked_size` (`src/decoder/body.rs:363`) における chunk-size 行は既にバイト列ベースで解析しており、ここは対象外である。

## AGENTS.md との関係

AGENTS.md L59 は「1 バイトずつ `as char` で String に push する経路は Latin-1 mojibake の原因になるため禁止」と規定している。本 issue が新設する `bytes_to_opaque_string` は形式上 `char::from_u32(b as u32)` で `0x80..=0xFF` を `U+0080..=U+00FF` にマップするが、これは AGENTS.md の禁止条項の **趣旨外** である。理由は以下の通り。

- AGENTS.md L59 の禁止が念頭に置く失敗 (closed/0059) は **valid UTF-8 として既知の `&str` を 1 バイトずつ `as char` で分解** することにより、`0xC3 0x80` のような valid な 2 バイトシーケンスを `U+00C3 U+0080` に潰すケース
- 本 issue の `bytes_to_opaque_string` は decoder 上流の `&[u8]` を入力とし、**先頭バイトを見て UTF-8 シーケンス長を判定 → valid なら `str::from_utf8` で Unicode scalar に変換 → invalid な単一バイトのみ `U+0080..=U+00FF` にマップ** する。valid UTF-8 シーケンスを Latin-1 に潰すケースは構造的に発生しない

ただし本 issue の方針は AGENTS.md L59 の文言と表面上衝突するため、本 issue 完了時に **AGENTS.md L59 を「`&str` 入力を valid UTF-8 として扱う場面に限定する」旨に書き換える** ことを完了条件に含める。

## 設計方針

1. request-line / status-line / header-line / trailer-line のパースを、行全体の UTF-8 強制解釈をやめてバイト列ベースに改修する。request-line / status-line 内の SP 位置はバイト列スキャンで求め、reason-phrase / field-value 部分を `bytes_to_opaque_string` に渡す。

   request-line の擬似コード:

   ```text
   let first_sp = line_bytes.iter().position(|&b| b == b' ').ok_or(...)?;
   let after_first = first_sp + 1;
   let second_sp = line_bytes[after_first..].iter().position(|&b| b == b' ')
       .map(|p| after_first + p).ok_or(...)?;
   let method = &line_bytes[..first_sp];
   let target = &line_bytes[after_first..second_sp];
   let version = &line_bytes[second_sp + 1..];
   // method は is_valid_token、version は is_valid_protocol_version、target は is_valid_request_target +
   // request.rs:382 の既存の non-ASCII 追加拒否で検証する。
   // request-line ABNF (RFC 9112 Section 3) は obs-text を許容しないため、3 個以上の SP を含む入力は
   // is_valid_request_target が SP を弾くことで自然に reject される (target に SP は許容されない)。
   ```

   status-line の擬似コード:

   ```text
   let first_sp = line_bytes.iter().position(|&b| b == b' ').ok_or(...)?;
   let after_first = first_sp + 1;
   let second_sp = line_bytes[after_first..].iter().position(|&b| b == b' ')
       .map(|p| after_first + p);
   let version = &line_bytes[..first_sp];
   let (status_bytes, reason_bytes): (&[u8], &[u8]) = match second_sp {
       Some(sp) => (&line_bytes[after_first..sp], &line_bytes[sp + 1..]),
       None => (&line_bytes[after_first..], b""), // reason-phrase 省略を許容
   };
   ```

   status-code は ASCII 数字 3 桁 (`100..=599`) としてバイト列で検証する。RFC 9112 Section 4 の ABNF `reason-phrase = 1*(...)` では reason-phrase は 1 文字以上だが、`status-line = HTTP-version SP status-code SP [ reason-phrase ]` でも reason-phrase 自体は optional である。実装上の寛容さとして、status-code 後の SP に続く値が空、または SP 自体が欠落している場合は reason-phrase absent として空文字列を許容する。

   `RequestDecoder` / `ResponseDecoder` が保持する `start_line: Option<String>` フィールドは削除する。代わりに専用 struct を導入する (中途半端な状態 (例: method だけ Some) が型上ありえないようにするため、3 つの個別 `Option<String>` ではなく 1 つの `Option<PendingStartLine>` とする)。

   ```rust
   struct PendingRequestStartLine { method: String, request_target: String, version: String }
   struct PendingResponseStartLine { version: String, status_code: u16, reason_phrase: String }
   ```

   `start_line` を参照している既存箇所 (`request.rs:412/415` version 取得 / `request.rs:467` method 取得 / `request.rs:510/513` Headers フェーズ再 split / `response.rs:387` `determine_body_kind` の version 取得 / `response.rs:509/512` status_code 抽出 / `response.rs:554/555` ResponseHead 構築) は、上記 struct のフィールドを参照するよう一括で書き換える。Headers フェーズで再度 `splitn(3, ' ')` している経路は完全に消え、`Vec<&str>::collect()` のヒープ確保も削減される。

2. obs-text オクテット (`0x80-0xFF`) は失わず、`String` 内に保持する。`src/decoder/utils.rs` を新設し、`bytes_to_opaque_string(bytes: &[u8]) -> Result<String, Error>` と `trim_ows_bytes(bytes: &[u8]) -> &[u8]` をそこに配置する。

   ```text
   1. 入力バイト列を先頭からインデックス i で走査する。
   2. i 位置の先頭バイトを見て、期待される UTF-8 シーケンス長 n (1, 2, 3, 4) を決定する。
      - 0x00-0x7F: n = 1
      - 0xC2-0xDF: n = 2
      - 0xE0-0xEF: n = 3
      - 0xF0-0xF4: n = 4
      - 0x80-0xBF / 0xC0-0xC1 / 0xF5-0xFF: invalid な先頭バイトとして n = 1 (1 バイトずつ obs-text として処理)
   3. n > 1 の場合、後続 n-1 バイトが continuation バイト (0x80-0xBF) かつシーケンス全体が RFC 3629 に従った valid UTF-8 (U+0000..=U+10FFFF、surrogate 除く、overlong エンコーディングを含まない) を表すかを `core::str::from_utf8(&bytes[i..i+n])` で検証する。
      - valid であれば対応する Unicode scalar の char を push し、i を n 進める。
      - invalid / incomplete / surrogate / overlong であれば、i 位置の 1 バイトのみを obs-text バイトとして処理する。
   4. 1 バイト処理時:
      - **0x00-0x1F の全 CTL (HTAB 0x09 を除く) および 0x7F (DEL) は Err(Error::InvalidData(...)) を返す**。RFC 9110 Section 5.5 は NUL/CR/LF を MUST reject、他の CTL を「MAY retain」とする。本実装は安全側に倒し、HTAB (`0x09`) 以外の全 CTL と DEL を一括 reject する。これにより `_opaque` validator が許容する文字集合 (HTAB / SP / VCHAR / U+0080..=U+10FFFF) と完全に一致し、2 段階チェックの矛盾は発生しない。
      - 0x20 (SP) / 0x21-0x7E (VCHAR) はそのまま char として push。
      - 0x09 (HTAB) はそのまま char として push。
      - 0x80-0xFF は char::from_u32(b as u32).unwrap() で U+0080..=U+00FF の char として push。
      - エラー variant は `Error::InvalidData(String)` を使う (新 variant 追加不要)。
   5. 全バイトを消費するまで繰り返す。
   6. 空入力 (`b""`) は Ok(String::new()) を返す。
   ```

   具体例:

   - `b""` → `""`
   - `b"\x80"` → `"\u{0080}"`
   - `b"\xFF"` → `"\u{00FF}"`
   - `b"\xC4\x80"` → `"\u{0100}"` (valid UTF-8)
   - `b"\xC3\x28"` → `"\u{00C3}("` (invalid sequence: 先頭バイトを obs-text、次を ASCII)
   - `b"\xC3"` → `"\u{00C3}"` (incomplete)
   - `b"\xED\xA0\x80"` → `"\u{00ED}\u{00A0}\u{0080}"` (UTF-8 として invalid な surrogate 範囲のバイト列を 1 バイトずつ処理)

   `bytes_to_opaque_string` は「decoder 上流の `&[u8]` を、UTF-8 不変条件を保ったまま Rust の `String` に変換する」decoder 専用関数である。本関数は (a) UTF-8 として valid なシーケンスを Unicode scalar に変換し、(b) HTTP として禁止される CTL (NUL/CR/LF を含む全 CTL、HTAB を除く) と DEL (0x7F) を reject し、(c) 1 バイト obs-text を `U+0080..=U+00FF` にマップする。受理する文字集合は `_opaque` 版 validator の `'\t' | ' ' | '!'..='~' | '\u{0080}'..='\u{10FFFF}'` と一致する。CTL reject の根拠は RFC 9110 Section 5.5「Field values containing CR, LF, or NUL ... MUST either reject the message or replace」と「Field values containing other CTL characters are also invalid」。

   性能について: 各先頭バイトで `core::str::from_utf8(&bytes[i..i+n])` を呼ぶ。`header_line` の hot path となるため、ASCII fast path (`is_ascii()` で全 ASCII 判定後にバルク変換) の最適化は follow-up とする (0107 で導入する criterion で計測のうえ実施)。本 issue では正しさを優先し、性能最適化は含めない。

3. `RequestHead` / `ResponseHead` 内部の文字列表現は `String` のまま維持する。`AGENTS.md` の方針に従い、`String` 内の各 `char` は「HTTP 上の 1 バイト obs-text として `U+0080..=U+00FF` にマップされたもの、または有効な UTF-8 マルチバイトシーケンス由来の Unicode scalar (`U+0000..=U+10FFFF`、surrogate 除く) のいずれか」という不変条件を満たす。

   ただしこの表現は元の HTTP バイト列を完全に復元できない。例えば HTTP 上の 1 バイト `0x80` と、valid UTF-8 シーケンス `0xC2 0x80` (U+0080) はどちらも `String` 内の 1 文字 `U+0080` になってしまい区別不能である。したがって `String::len()` は HTTP バイト長と一致せず、proxy 透過・Content-Length 整合性に影響する。本 issue では受理経路のみ修正し、透過送出は別 issue (本 issue 完了条件 follow-up) で扱う。

4. `src/validate.rs` の `is_valid_field_value` (`src/validate.rs:63`) / `is_valid_reason_phrase` (`src/validate.rs:143`) は builder 用に既存の byte 判定のまま維持する。decoder 専用に以下を新設する。

   - `is_valid_field_value_opaque(value: &str) -> bool`: `value.chars().all(|c| matches!(c, '\t' | ' ' | '!'..='~' | '\u{0080}'..='\u{10FFFF}'))`
   - `is_valid_reason_phrase_opaque(phrase: &str) -> bool`: 同上

   空の reason-phrase は absent 扱いとして許容するが、これは validator ではなく呼び出し側で制御する (status-line パース時に判断)。

5. `parse_header_line` を `&[u8]` 対応にする。`find_line` (`src/decoder/body.rs:733`) は既に `&[u8]` 対応なので変更不要。新しい signature と内部フローは以下の通り。`trim_ows_bytes` は `pub(crate) fn trim_ows_bytes(bytes: &[u8]) -> &[u8]` とし、`trim_ows` (`src/validate.rs:396`) と同様に先頭・末尾の SP / HTAB のみをスキップする byte ベース版とする。配置先は `src/decoder/utils.rs`。

   ```text
   pub(crate) fn parse_header_line(line: &[u8]) -> Result<(String, String), Error> {
       // 1. 空行チェック。空なら invalid header line: empty
       // 2. 行頭 obs-fold 拒否 (line[0] == b' ' || line[0] == b'\t')  RFC 9112 Section 5.2 準拠
       // 3. 行内の NUL / CR / LF 拒否 (bytes_to_opaque_string 内でも reject されるが、name 区間で早期検知)
       // 4. 最初の ':' バイトで name / value を分割。':' がなければ reject
       // 5. name バイト列を is_valid_header_name (byte ベース) で検証し、ASCII token なので安全に str に変換
       // 6. value 部分を trim_ows_bytes で前後 OWS 除去
       // 7. 除去後の value バイト列を bytes_to_opaque_string で String 化
       // 8. is_valid_field_value_opaque で検証
       // 9. Ok((name.to_string(), value_string))
   }
   ```

6. セマンティックヘッダー値 (`Content-Length` / `Transfer-Encoding` / `Host` / `Connection` / `Trailer` 等) に obs-text が混入した場合は、HTTP Request Smuggling / Response Splitting 対策として構文エラーとする。これらの値は `parse_header_line` 後の `String` として個別パーサー (`parse_content_length_value` / `parse_transfer_encoding_*` / `collect_declared_trailers` 等) に渡されるが、これらのパーサーは ASCII 数字・token・OWS のみを受理するため、obs-text が含まれれば自然にエラーとなる。テストで `Content-Length: 10\x80` のような入力が reject されることを確認する。

7. `src/auth.rs` / `src/content_disposition.rs` の quoted-string パーサーは `&str` 版のまま変更しない。decoder 側で field-value 全体を `bytes_to_opaque_string` で valid UTF-8 `String` に変換してから `Authorization::parse` / `ContentDisposition::parse` に `&str` として渡すことで、quoted-string パーサーは RFC 9112 Section 2.2 が許容する「protocol element 抽出後の文字列処理」として動作する。

   closed/0059 (`auth.rs` Latin-1 mojibake 修正) との関係: closed/0059 が前提とした「`String` 内の `U+0080..=U+00FF` は **常に** 1 バイト obs-text 由来」という単射性は、本 issue 適用後は崩れる (valid UTF-8 シーケンス `0xC2 0x80` も `U+0080` を生成するため)。ただし `auth.rs` / `content_disposition.rs` は `U+0080..=U+00FF` を **不透明な char として** quoted-string 中で受理するのみで、それを 1 バイトに戻す処理は持たない (`is_qdtext_char` / `is_quoted_pair_char` の char マッチで処理が完結する)。下流で「同じ `U+0080` が異なる入力由来でも区別不要」であることを実コードで確認した上で、本 issue は単射性破壊を許容する。

8. `is_valid_request_target` (`src/validate.rs:172`) は **変更しない**。decoder 側の non-ASCII 追加拒否は `request.rs:382` で既に実装済み (`parts[1].bytes().any(|b| b >= 0x80)` 相当) のためそのまま維持する。本 issue で新規追加するロジックではない。rustdoc には「歴史的互換性のため builder 用 validator は obs-text を許容するが、decoder 側で non-ASCII を追加拒否する」旨を明記する。

9. `RequestHead::from_validated_parts` / `ResponseHead::from_validated_parts` の `debug_assert!` を以下のように変更する。これらは `pub(crate)` で外部影響なし。併せて rustdoc も更新する。

   `RequestHead::from_validated_parts` (`pub(crate) fn` 宣言は `src/decoder/head.rs:314`、`debug_assert!` は `src/decoder/head.rs:329`):

   - `is_valid_request_target(&uri)`: 現状維持。decoder 側で request-target の non-ASCII 追加拒否を行う旨を rustdoc に明記する。
   - `is_valid_protocol_version`: 変更なし。
   - header values を `is_valid_field_value_opaque` に変更。

   `ResponseHead::from_validated_parts` (`pub(crate) fn` 宣言は `src/decoder/head.rs:519`、`debug_assert!` は `src/decoder/head.rs:538`):

   - `is_valid_protocol_version`: 変更なし。
   - `is_valid_status_code`: 変更なし。
   - reason_phrase を `(empty || is_valid_reason_phrase_opaque(...))` に変更。
   - header values を `is_valid_field_value_opaque` に変更。

10. 送信側 builder / encoder については本 issue では扱わない。`String` 内の `U+0080..=U+00FF` を `String::as_bytes()` すると UTF-8 化されて元の obs-text 1 バイトと異なるため、`examples/http11_reverse_proxy` のような透明転送では別途対応が必要。完全な透明転送 (内部表現を `Vec<u8>` 化するか、encoder 側で `U+0080..=U+00FF` → 1 バイト obs-text に変換するか) は follow-up issue で議論する。

11. RTSP/1.0 や RTSP/2.0 でも同様のメッセージ構文を処理するため、本変更は HTTP/1.1 に限らず RTSP の start-line / header-line でも適用される。

12. 0121 (`refactor-requesthead-responsehead-string-api`) との関係: 0121 は `RequestHead::new` / `ResponseHead::new` の引数型を `&str` から `impl Into<String>` 等に変更する内部 API リファクタで、本 issue が触る decoder 内部状態 (`start_line` 削除と個別フィールド化) には直接影響しない。本 issue と 0121 は独立して進められる。

## 完了条件

- obs-text (`0x80-0xFF`) を含む reason-phrase / field-value / chunked trailer を持つ有効な HTTP/1.1 メッセージが `RequestDecoder` / `ResponseDecoder` で受理されるようになること。
- 以下の具体例を含むデコードテストを `tests/test_decoder/head.rs` / `tests/test_decoder/body.rs` に追加すること。
  - 空入力 (`b""`) に対する `bytes_to_opaque_string` が `Ok(String::new())` を返すこと
  - reason-phrase に `0x80` / `0xFF` を含むレスポンス
  - field-value に `0x80` / `0xFF` を含むリクエストとレスポンス
  - chunked trailer の field-value に `0x80` / `0xFF` を含むメッセージ
  - 有効な UTF-8 マルチバイト (例: `U+0100`) と obs-text の混在
  - incomplete UTF-8 シーケンス (例: `0xC3` だけ、`0xF0 0x90` だけ) を含む field-value / reason-phrase
  - `Content-Length: 10\x80` のようなセマンティック値に obs-text が混入したメッセージが拒否されること
- NUL / CR / LF / DEL (0x7F) を含む field-value / reason-phrase / chunked trailer は引き続き拒否されること (RFC 9110 Section 5.5 / RFC 9112 Section 4 準拠)。
- method (RFC 9112 Section 3.1 / Appendix A `token`) / request-target (RFC 3986 Section 2.1-2.3、RFC 9112 Section 3.2) / HTTP-version (RFC 9112 Section 2.3) / header-name (RFC 9110 Section 5.1 `token`) に `0x80-0xFF` を含むメッセージは引き続き拒否されること。
- 既存の UTF-8 only メッセージのデコード挙動が変わらないこと。
- 以下の既存 PBT を更新すること。
  - `pbt/tests/prop_decoder/request.rs`
    - `prop_invalid_utf8_request_line_error`: request-target / method / HTTP-version に obs-text を含むため、**引き続き拒否** されるケースに名称変更・整理する。例: `prop_obs_text_in_request_line_still_rejected`。
    - `prop_invalid_utf8_header_error`: ヘッダー値の obs-text を **受理** する方向に反転する。`decode_headers().is_ok()` を確認し、`head.get_header("X-Obs")` が `U+0080` / `U+00FF` 等を含むことを `assert_eq!` で検証する。
  - `pbt/tests/prop_decoder/response/status_line.rs`
    - `prop_invalid_utf8_status_line_error`: reason-phrase の obs-text を **受理** する方向に反転する。`decode_headers().is_ok()` を確認し、`head.reason_phrase()` が `U+0080` / `U+00FF` 等を含むことを検証する。
    - `prop_invalid_utf8_response_header_error`: ヘッダー値の obs-text を **受理** する方向に反転する。`head.get_header("X-Obs")` の内容を検証する。
  - 新規に「引き続き拒否」ケースを追加する: request-target / header-name / method / HTTP-version に obs-text を含むケース。入力として単一 obs-text バイト `0x80` / `0xFF`、有効な UTF-8 マルチバイト `0xC4 0x80` (U+0100) と obs-text の混在、incomplete UTF-8 シーケンス `0xC3` のみ、`0xF0 0x90` のみを含める。
  - closed/0086 で `pbt/src/lib.rs` の `field_vchar()` 戦略が obs-text を含むよう統合済みのため、本 issue で新規 obs-text strategy は追加しない。
- 以下の fuzz target を確認・必要に応じて修正すること。fuzz target に固定 obs-text 入力を埋め込むのは fuzz の趣旨と外れるため、固定ケース確認は `tests/test_decoder/` 側に置く。
  - `fuzz_decoder_request` / `fuzz_decoder_response` (`fuzz/fuzz_targets/fuzz_decoder_request.rs` / `fuzz_decoder_response.rs`): 入力分布に obs-text バイトが出現するよう生成器を点検する。固定 obs-text シナリオは fuzz でなく単体テストに置く。
  - `fuzz_chunked_trailer` (`fuzz/fuzz_targets/fuzz_chunked_trailer.rs`): `FuzzInput.trailers` の value 型を `Vec<u8>` に変更し、`Vec<(String, Vec<u8>)>` とする。obs-text 単一バイト (`0x80-0xFF`) を直接バイト列に含めるように修正。`normalize_trailers` 内の `is_valid_value` を byte ベースに変更し、`build_trailer_section` で `Vec<u8>` 値をそのままバイト列に書き込む。
- `src/validate.rs` の `is_valid_field_value` (`src/validate.rs:63`) / `is_valid_reason_phrase` (`src/validate.rs:143`) のコメントを「builder 用。入力は valid UTF-8 のみを想定しており、HTTP 上の 1 バイト obs-text (0x80-0xFF) は `String` 内では UTF-8 マルチバイト (例: U+0080 → 0xC2 0x80) として表現されるが、この関数は ABNF 準拠の `VCHAR / obs-text` を byte 値で判定するため、builder からの obs-text は実質的に受理できない」という意味に更新する。
- `src/validate.rs` の `is_valid_request_target` (`src/validate.rs:172`) の rustdoc / コメントを更新し、「構文上は obs-text を含まないが歴史的互換性のため builder では許容している。decoder 側では RFC 9112 Section 3.2 / RFC 3986 に従い non-ASCII を追加拒否する (`request.rs:382` の既存ロジック)」旨を明記する。
- `src/decoder/head.rs` の `RequestHead::from_validated_parts` (`src/decoder/head.rs:314` 宣言、`debug_assert!` は `head.rs:329`) / `ResponseHead::from_validated_parts` (`head.rs:519` 宣言、`debug_assert!` は `head.rs:538`) の rustdoc を更新し、`_opaque` 版 validator の使用と decoder 側での request-target 追加拒否を明記する。
- `src/decoder/request.rs` / `src/decoder/response.rs` / `src/decoder/body.rs` のモジュール先頭コメント (それぞれ先頭 11 行程度) を更新し、UTF-8 強制解析の非準拠を削除する。
- `README.md` の「既知の制限事項」(`README.md:434`) を更新する。obs-text は Unicode scalar 拡張解釈で受理・保持されるが、`String::as_bytes()` すると元の 1 バイトとは異なる UTF-8 表現となること、完全な透明転送が必要な場合は内部表現変更または encoder 側対応が必要である旨を追記する。
- `AGENTS.md` の L59「1 バイトずつ `as char` で String に push する経路は禁止」を、以下のように 2 行に分けて書き換える。本 issue 実装 PR と **同 PR** で AGENTS.md も更新する (実装と規約の同時整合性を保つため、別 issue 化はしない)。L58 の「受信時は」スコープと L60 の「char 単位走査」前提を維持する。
  - 改訂文 (L59): 「受信時は `char_indices()` ベースで走査し UTF-8 不変条件を保つこと (valid UTF-8 として既知の `&str` 入力を 1 バイトずつ `as char` で String に push する経路は Latin-1 mojibake の原因になるため禁止)」
  - 改訂文 (L59 補足、新規追加): 「decoder 上流の `&[u8]` → `String` 変換は `&str` 入力ではなく未検証のオクテット列を扱うため、本禁止条項の対象外。専用関数 `bytes_to_opaque_string` (`src/decoder/utils.rs`) を経由する」
- 完全な透明転送を実現するための follow-up issue を作成し、その番号を本 issue 完了条件として記録する。
- `CHANGES.md` の `## develop` セクションに以下の `[FIX]` エントリを `[ADD]` の下、`### misc` の上に追加する (shiguredo-changelog 規約: CHANGE → ADD → UPDATE → FIX の順)。全角括弧ではなく半角括弧を使う。
  - `[FIX] decoder が reason-phrase / field-value / chunked trailer の obs-text (0x80-0xFF) を UTF-8 強制解釈で拒否していた問題を修正し、obs-text を Unicode scalar 拡張解釈 (U+0080..=U+00FF) で受理・保持する。`

## 解決方法

設計方針に従い、以下を実装する (follow-up issue 作成と AGENTS.md L59 改訂は完了条件側で管理するため、ここからは省く)。

- `src/decoder/request.rs` / `src/decoder/response.rs` / `src/decoder/body.rs` の `String::from_utf8` による行解析をバイト列ベースの解析に置き換える。`RequestDecoder` / `ResponseDecoder` の `start_line: Option<String>` フィールドを削除し、設計方針 1 で挙げた個別フィールド (request 側: `method` / `request_target` / `version`、response 側: `version` / `status_code` / `reason_phrase`) に置き換える。Headers フェーズで再度 split している経路 (request.rs:510/513、response.rs:509-512/554-555) は個別フィールド参照に書き換える。
- `src/decoder/body.rs` の `parse_header_line` を `&[u8]` 対応に変更する。
- `src/decoder/utils.rs` を新設し、`bytes_to_opaque_string` / `trim_ows_bytes` を配置する。
- `src/decoder/mod.rs` に `mod utils;` を追加し、`request.rs` / `response.rs` / `body.rs` から `utils` モジュールを参照する。
- `src/validate.rs` に `is_valid_field_value_opaque` / `is_valid_reason_phrase_opaque` を新設する。
- `src/decoder/head.rs` の `RequestHead::from_validated_parts` / `ResponseHead::from_validated_parts` の `debug_assert!` と rustdoc を `_opaque` 版 validator を使うように変更する。
- `src/auth.rs` / `src/content_disposition.rs` は `&str` 版のまま変更せず、decoder 側で field-value 全体を変換してから渡す。
- 既存 PBT / fuzz target / 単体テストを更新する。
- `README.md` / モジュールコメント / `is_valid_field_value` / `is_valid_reason_phrase` のコメントを更新する。

## 参考: RFC 文面

- RFC 9110 Section 5.1
  > field-name = token
- RFC 9110 Section 5.5
  > field-value = *field-content
  > field-content = field-vchar [ 1*( SP / HTAB / field-vchar ) field-vchar ]
  > field-vchar = VCHAR / obs-text
  > obs-text = %x80-FF
  > Field values are usually constrained to the range of US-ASCII characters. Fields needing a greater range of characters can use an encoding such as the one defined in [RFC8187].
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
  > Recipients that process the value of a quoted-string MUST handle a quoted-pair as if it were replaced by the octet following the backslash.
- RFC 9112 Section 2.2
  > A recipient MUST parse an HTTP message as a sequence of octets in an encoding that is a superset of US-ASCII [USASCII].
  > Parsing an HTTP message as a stream of Unicode characters, without regard for the specific encoding, creates security vulnerabilities due to the varying ways that string processing libraries handle invalid multibyte character sequences that contain the octet LF (%x0A).
  > Although the line terminator for the start-line and fields is the sequence CRLF, a recipient MAY recognize a single LF as a line terminator and ignore any preceding CR.
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
