# 0010: validate.rs のモジュール責務を整理する

Created: 2026-04-28
Model: Kimi 2.6 / GPT 5.5 / Composer 2 Fast

## 概要

`src/validate.rs` に RFC 基本文字集合の共通検証とエンコード専用ポリシーが混在しており、モジュール責務が曖昧になっている。責務を明確に分離する。

## 根拠

現状の `src/validate.rs` には以下の混在がある。

| 関数 | 用途 | 呼び出し元 |
| ---- | ---- | ---------- |
| `is_token_char` | 基本文字集合 | `validate.rs` 内部 |
| `is_valid_header_name` | 基本文字集合 | `encoder.rs`, `decoder/body.rs` |
| `is_valid_token` | 基本文字集合 | `decoder/body.rs` |
| `is_valid_field_vchar` | 基本文字集合 | `validate.rs` 内部 |
| `is_valid_field_value` | 基本文字集合 | `encoder.rs`, `decoder/body.rs` |
| `is_valid_method` | 基本文字集合 | `encoder.rs`, `decoder/request.rs` |
| `is_valid_protocol_version` | 基本文字集合 | `decoder/request.rs`, `decoder/response.rs` |
| `is_valid_version_for_encode` | **エンコード専用ポリシー** | `encoder.rs` |
| `is_valid_status_code` | 基本文字集合 | `encoder.rs`, `decoder/response.rs` |
| `is_valid_reason_phrase` | 基本文字集合 | `encoder.rs`, `decoder/response.rs` |
| `is_valid_request_target_for_decode` | 基本文字集合 + 受信寛容 | `decoder/request.rs` |
| `is_valid_request_target_for_encode` | 基本文字集合 + 送信厳格（エンコード専用） | `encoder.rs` |
| `is_pchar_or_slash` 等 | 基本文字集合 | `validate.rs` 内部, `decoder/body.rs` |

**注:** 0008 で `is_valid_request_target` が `for_decode` / `for_encode` に分離される。本 issue では `is_valid_version_for_encode` のみをエンコーダー側に移動する。

`is_valid_version_for_encode` は「HTTP-version ではなく任意のプロトコル識別子（RTSP 等）に対して VCHAR のみを許容する」というエンコード側のポリシーであり、デコード側の `is_valid_protocol_version`（HTTP/RTSP の厳格な形式検証）とは責務が異なる。この関数が `validate.rs` に置かれていることで、モジュールの責務が「RFC 基本文字集合検証」か「エンコードポリシー」か曖昧になっている。

## 対象ファイルと変更点

### `src/encoder.rs`

1. `src/encoder.rs` 内に `mod validate` を追加するか、`src/encoder/validate.rs` を新設する。
2. `is_valid_version_for_encode` をエンコーダーモジュール内に移動する。

   ```rust
   // src/encoder.rs 内または src/encoder/validate.rs
   /// エンコード用のバージョン文字列バリデーション
   ///
   /// VCHAR のみ (SP/CTL 禁止)。RTSP 等の非 HTTP プロトコルにも対応。
   pub(crate) fn is_valid_version_for_encode(version: &str) -> bool {
       !version.is_empty() && version.bytes().all(|b| matches!(b, 0x21..=0x7E))
   }
   ```

3. `src/encoder.rs` の `use crate::validate::...` から `is_valid_version_for_encode` を削除する。

### `src/validate.rs`

1. `is_valid_version_for_encode` を削除する。
2. モジュール doc comment を「RFC 9110 / RFC 3986 基本文字集合の共通検証（デコード・エンコード双方で使用）」に更新する。

### `src/lib.rs`

変更不要（`validate` モジュールは非公開のまま）。

## 影響範囲

- `is_valid_version_for_encode` の移動は `pub(crate)` なので crate 外への影響はない。
- `encoder.rs` の内部構造が少し変わるのみ。

## 検証

- `make fmt && make clippy && make check && make test` を通す。
- エンコーダーのバリデーションロジックが挙変していないことを確認する。
- `is_valid_version_for_encode` が `encoder.rs` からのみ呼ばれていることを確認する。
