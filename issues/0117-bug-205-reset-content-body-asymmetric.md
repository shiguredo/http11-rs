# 205 Reset Content のボディ扱いが encoder/decoder で非対称

- Priority: High
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-205-reset-content-body-asymmetric
- Polished: 2026-06-16

## 目的

`205 Reset Content` のボディ扱いをエンコーダーとデコーダーで対称にし、RFC 9110 Section 15.3.6 に準拠する。

## 優先度根拠

High とする。RFC 9110 Section 15.3.6 では "a server MUST NOT generate content in a 205 response" と規定されている。現状エンコーダーは 205 に対してボディ / TE / 非ゼロ Content-Length を拒否するが、デコーダーは 205 を通常の 2xx と同様に扱うため、非対称が生じている。

## 現状

- `src/encoder.rs:727-743` : `encode_response` で 205 に対し非空ボディ / Transfer-Encoding / 非ゼロ Content-Length を拒否する。
- `src/encoder.rs:1016-1030` : `encode_response_headers` でも同様に Transfer-Encoding / 非ゼロ Content-Length を拒否する。
- `src/encoder.rs:88-90` : `response_status_has_body` は 1xx / 204 / 304 のみを「ボディなし」と判定し、205 は「ボディあり」と扱う。
- `src/decoder/response.rs:323-326` : `status_has_body` が 205 を「ボディあり」と判定する。
- `src/decoder/response.rs:338-426` : `determine_body_kind` で 205 に対し Transfer-Encoding / Content-Length フレーミングを適用可能。

## 設計方針

### RFC 解釈の独自拡張と過去判断の修正

RFC 9112 Section 6.3 item 1 は 1xx / 204 / 304 のみを「ボディなし」と規定し、**205 は含まれない**。RFC 9110 Section 15.3.6 の "a server MUST NOT generate content in a 205 response" は文言上 **送信者制約のみ** で、受信者の挙動は規定されていない。一方、closed/0016 では「205 の扱い: 送信者制約のみ。受信側は `status_has_body` で true 扱いとし、TE/CL に従う。**現状維持**」と方針決定済み。

本 issue ではこの過去判断を **意図的に逆転** する。理由:

- AGENTS.md「Request / Response / RequestHead / ResponseHead 間で API の一貫性を保つこと」と、現状の encoder/decoder 非対称性 (encoder は 205 で非ゼロボディ / TE 拒否、decoder は通常受信) が直接矛盾する
- 防御的に MUST NOT を受信側にも適用することで、不正な 205 ボディを下流に流さない安全側設計となる (HRS 経路の予防)
- RFC 9112 §6.3 item 1 を機械的に守るより、9110 §15.3.6 の意図 (205 はボディを持たない) を優先する。これは本ライブラリの設計選択であり、本 issue で文書化する

### 具体方針

1. 205 のボディなし扱いをエンコーダーとデコーダーで **完全対称** にする。具体的には以下 3 関数すべてで 205 を「ボディなし」として扱う:
   - `src/encoder.rs:88` `response_status_has_body` に 205 を含める (現状: 1xx / 204 / 304 のみ → 変更後: 1xx / 204 / 205 / 304)
   - `src/decoder/response.rs:323` `status_has_body` に 205 を含める (現状: 1xx / 204 / 304 のみ → 変更後: 1xx / 204 / 205 / 304)
   - `src/decoder/response.rs:338` `determine_body_kind` で 205 を特別扱い (`status_has_body` 変更により自動的に到達)

2. エンコーダー側の既存の 205 検証 (`encode_response` L727-743、`encode_response_headers` L1016-1030) は維持する。`response_status_has_body` に 205 を含める変更により、Content-Length 自動付与の挙動が変わる (205 では自動付与しない) が、これは 205 で非ゼロボディを拒否する既存挙動と整合する。Content-Length: 0 の許容も既存検証で維持する。

3. デコーダー側では、205 に対して `Transfer-Encoding` / 非ゼロ `Content-Length` が存在する場合をエラーとする。`Content-Length: 0` または `Content-Length` / `Transfer-Encoding` なしの場合は `BodyKind::None` を返してヘッダー終了で完了とする (encoder と整合)。

4. `determine_body_kind` 内の現状コメント (`L342-344` 付近の「205 は status_has_body が true を返すためここではマッチせず、後続の...」) を更新し、本 issue の方針変更を反映する。

5. closed/0016 の方針逆転は本 issue でのみ扱い、過去 issue ファイル (`issues/closed/0016-*.md`) は履歴として変更しない (closed/ は git 履歴の一部、書き換えは不適切)。

## 完了条件

- `src/encoder.rs:88` の `response_status_has_body` に 205 を含めること (1xx / 204 / 205 / 304 を「ボディなし」と判定)。
- `src/decoder/response.rs:323` の `status_has_body` に 205 を含めること (同上)。
- `src/decoder/response.rs:338` の `determine_body_kind` で 205 に対し非ゼロ `Content-Length` / `Transfer-Encoding` の存在をエラーにすること。`Content-Length: 0` または両者なしの場合は `BodyKind::None` を返すこと。
- `src/decoder/response.rs:342-344` 周辺のコメント (現在「205 は status_has_body が true を返す」と書かれている部分) を本 issue の方針変更を反映する内容に更新すること。
- エンコーダー側の 205 既存検証 (`encode_response` L727-743、`encode_response_headers` L1016-1030) が維持されること。
- `tests/test_decoder/body.rs` に 205 の BodyKind 判定テストを追加すること。
  - `Transfer-Encoding: chunked` がある 205 をエラーにするテスト
  - 非ゼロ `Content-Length` がある 205 をエラーにするテスト
  - `Content-Length: 0` がある 205 を `BodyKind::None` として受理するテスト
  - `Transfer-Encoding` / `Content-Length` のない 205 を `BodyKind::None` として受理するテスト
- `tests/test_encoder/` (該当する既存ファイル) に 205 が `response_status_has_body` 変更後も自動 Content-Length 付与をスキップし、ボディなしで encode されることを検証するテストを追加すること。
- `CHANGES.md` の `## develop` セクションに `[FIX]` エントリとして以下を追加すること。`[ADD]` の下、`### misc` の上に配置する。
  - `[FIX] 205 Reset Content のボディ扱いを encoder/decoder で対称化し、RFC 9110 Section 15.3.6 の "MUST NOT generate content" を受信側でも防御的に適用する (closed/0016 の現状維持方針を意図的に逆転)。`

## 解決方法

設計方針に沿って次の順序で実装する。

1. `src/encoder.rs:88` の `response_status_has_body` に 205 を含める (1xx / 204 / 205 / 304 を「ボディなし」と判定)。
2. `src/decoder/response.rs:323` の `status_has_body` に 205 を含める。
3. `src/decoder/response.rs:338` の `determine_body_kind` で 205 ステータスに対し、非ゼロ `Content-Length` / `Transfer-Encoding` の存在をエラーとし、それ以外は `BodyKind::None` を返す経路を実装する。
4. `src/decoder/response.rs:342-344` 周辺のコメントを本 issue の方針変更を反映する内容に更新する。
5. 完了条件のテストを `tests/test_decoder/body.rs` / `tests/test_encoder/` に追加する。
6. `CHANGES.md` に `[FIX]` エントリを追加する。

## 参考: RFC 文面

- RFC 9110 Section 15.3.6 205 Reset Content
  > The 205 (Reset Content) status code indicates that the server has successfully fulfilled the request and desires that the user agent reset the "document view", which caused the request to be sent, to its original state as received from the origin server.
  > Since the 205 status code implies that no additional content will be provided, a server MUST NOT generate content in a 205 response.
- RFC 9112 Section 6.3 item 1
  > Any response to a HEAD request and any response with a 1xx (Informational), 204 (No Content), or 304 (Not Modified) status code is always terminated by the first empty line after the header section, regardless of the header fields present in the message, and thus cannot contain a message body or trailer section.
