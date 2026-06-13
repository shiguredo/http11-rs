# 205 Reset Content のボディ扱いが encoder/decoder で非対称

- Priority: High
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-205-reset-content-body-asymmetric
- Polished: 2026-06-13

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

1. RFC 9112 Section 6.3 item 1 は 1xx / 204 / 304 のみを「ボディなし」と規定しており、205 は含まれない。しかし、RFC 9110 Section 15.3.6 の "MUST NOT generate content" を受信側でも防御的に適用するため、デコーダー側でも 205 を特別扱いする。
2. エンコーダー側の既存の 205 検証は維持する。`response_status_has_body` に 205 を含める変更は、Content-Length 自動付与やボディ長整合性検証の挙動にも影響を与えるため、別途検討する。
3. デコーダー側では、205 に対して Transfer-Encoding / Content-Length が存在する場合をエラーとする。存在しない場合は `BodyKind::None` を返してヘッダー終了で完了とする。
4. 両者の挙動を一致させる。

## 完了条件

- `src/decoder/response.rs` の `determine_body_kind` / `status_has_body` で 205 を特別扱いし、Transfer-Encoding / Content-Length の存在をエラーにすること。
- 205 に Transfer-Encoding / Content-Length がない場合、`BodyKind::None` を返すこと。
- エンコーダー側の 205 検証が維持されること。
- `tests/test_decoder/body.rs` に 205 の BodyKind 判定テストを追加すること。
  - Transfer-Encoding: chunked がある 205 をエラーにするテスト
  - Content-Length: 非ゼロ がある 205 をエラーにするテスト
  - Content-Length: 0 がある 205 を `BodyKind::None` として受理するテスト
  - Transfer-Encoding / Content-Length のない 205 を `BodyKind::None` として受理するテスト
- `CHANGES.md` に本変更を記載すること。

## 解決方法

- `src/decoder/response.rs` の `determine_body_kind` / `status_has_body` で 205 を特別扱いする。
- 必要に応じて `BodyKind::None` を返すか、エラーを返す。
- テストを追加する。
