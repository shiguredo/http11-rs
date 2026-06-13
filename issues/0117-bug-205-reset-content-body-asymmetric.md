# 205 Reset Content のボディ扱いが encoder/decoder で非対称

- Priority: High
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-205-reset-content-body-asymmetric
- Polished: {YYYY-MM-DD}

## 目的

`205 Reset Content` のボディ扱いをエンコーダーとデコーダーで対称にし、RFC 9110 Section 15.3.6 に準拠する。

## 優先度根拠

High とする。RFC 9110 Section 15.3.6 では "a server MUST NOT generate content in a 205 response" と規定されている。現状エンコーダーは 205 に対してボディ / TE / 非ゼロ CL を拒否するが、デコーダーは 205 を通常の 2xx と同様に扱うため、非対称が生じている。

## 現状

- `src/encoder.rs:727-743` : 205 ではボディ / TE / 非ゼロ CL を拒否。
- `src/decoder/response.rs:323-326` : `status_has_body` が 205 を「ボディあり」と判定。
- `src/decoder/response.rs:338-426` : `determine_body_kind` で 205 に対し Transfer-Encoding / Content-Length フレーミングを適用可能。

## 設計方針

1. デコーダー側でも 205 を 204 と同様に「コンテンツなし」として扱うか、Transfer-Encoding / Content-Length の存在をエラーにする。
2. エンコーダー側の既存の 205 検証は維持する。
3. 両者の挙動を一致させる。

## 完了条件

- デコーダーが 205 レスポンスに対してボディを読まず、または TE/CL 存在をエラーにすること。
- エンコーダーの 205 検証が維持されること。
- テストで対称性が検証されること。

## 解決方法

- `src/decoder/response.rs` の `determine_body_kind` / `status_has_body` で 205 を特別扱いする。
- 必要に応じて `BodyKind::None` を返すか、エラーを返す。
- テストを追加する。
