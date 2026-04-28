# 0005: RequestTargetForm の共通モジュール化

Created: 2026-04-28
Model: Opus 4.7 (1M context)

## 概要

`request-target` の形式を表す `RequestTargetForm` enum が `src/encoder.rs` と `src/decoder/body.rs` の双方で重複定義されている。RFC 9112 Section 3.2 で定義される encoder / decoder 共通の概念であり、二重定義を解消する。

## 重複箇所

| 項目 | `src/encoder.rs` | `src/decoder/body.rs` |
| ---- | ---------------- | --------------------- |
| `enum RequestTargetForm` | L150-L159 | L680-L693 |
| 可視性 | private | `pub` |
| derive | なし | `Debug, Clone, Copy, PartialEq, Eq` |
| バリアント | `Origin` / `Absolute` / `Authority` / `Asterisk` | 同左 |

両者ともバリアント名・意味は完全に一致している。一方で関連関数の責務が異なる。

| 関数 | 場所 | 責務 |
| ---- | ---- | ---- |
| `detect_request_target_form` | `src/encoder.rs` L65 | 形式判定のみ |
| `parse_request_target_form` | `src/decoder/body.rs` L702 | 形式判定 + バリデーション |
| `validate_request_target_for_method` | `src/decoder/body.rs` L1107 | メソッドと形式の整合性検証 (decoder 専用) |

## 対応が必要な根拠

- RFC 9112 Section 3.2 の同一概念が二重定義されており、将来 RFC 改定や仕様解釈の修正があった場合に片方だけ修正するリスクが生じる。
- decoder と encoder で形式分類が将来ズレた場合、decode 後に再 encode するラウンドトリップで挙動の不一致が起きる可能性がある。
- 現状は派生属性が片方にしかなく、encoder 側で `RequestTargetForm` を比較・複製したくなった場合に追加実装が必要になる。共通化すれば派生属性も統一できる。

## 対応方針

- `src/request_target.rs` を新規作成し、`enum RequestTargetForm` を移動する。
  - 派生属性は `Debug, Clone, Copy, PartialEq, Eq` に統一する。
  - 可視性は `pub` (request-target の形式は外部からも参照する公開 API として扱う)。
- `src/lib.rs` に `pub mod request_target;` を追加する。
- `src/encoder.rs` 内の重複 enum を削除し、`use crate::request_target::RequestTargetForm;` で参照する。
- `src/decoder/body.rs` 内の重複 enum を削除し、`use crate::request_target::RequestTargetForm;` で参照する。`decoder::body::RequestTargetForm` のパスは廃止する。
- 関数は分離維持する。
  - encoder 側の `detect_request_target_form` (形式判定のみ) はそのまま `src/encoder.rs` に残す。
  - decoder 側の `parse_request_target_form` (バリデーション付き) はそのまま `src/decoder/body.rs` に残す。
  - `validate_request_target_for_method` も decoder 専用なので移動しない。

## 後方互換

- 後方互換は維持しない。`decoder::body::RequestTargetForm` の参照パスは `request_target::RequestTargetForm` に変更する。再エクスポートでパスを温存するとメンテナンス性が下がるため、保守的な再エクスポートはしない。
- 利用側は import パスを `use shiguredo_http11::request_target::RequestTargetForm;` に変更する。CHANGES に破壊的変更として記載する。
- encoder 側の enum は private だったため、外部影響はない。

## 検証

- `make fmt && make clippy && make check && make test` を通す。
- 既存の encoder / decoder のテストがそのまま緑であることを確認する。
- ラウンドトリップ系の PBT (`pbt/tests/prop_*`) が壊れていないことを確認する。
