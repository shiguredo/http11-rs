# 0066 refactor 長大テストファイルを分割する

Created: 2026-05-14
Model: deepseek-v4-pro

## 概要

以下のテストファイルが CLAUDE.md:97「テストファイルが長くなった場合はファイル内で mod を使って分割すること」に違反する規模になっている:

- `tests/test_decoder.rs` (1907 行)
- `pbt/tests/prop_decoder/response.rs` (1317 行)

両ファイルとも複数の独立したテストグループ (status line parsing, body decoding, limits, streaming, keep-alive, 1xx handling 等) を含んでおり、ディレクトリモジュールへの分割が適切。

## 対象ファイル

- `tests/test_decoder.rs` → `tests/test_decoder/` ディレクトリモジュールに分割
- `pbt/tests/prop_decoder/response.rs` → `pbt/tests/prop_decoder/response/` にサブモジュール分割

## 推奨対応

1. `tests/test_decoder/` ディレクトリを作成し `main.rs` + `head.rs` + `body.rs` + `streaming.rs` + `direct_buffer.rs` 等に分割する
2. 同時に `tests/test_decode_body.rs` (命名規則違反、0065 参照) を `tests/test_decoder/body.rs` の一部として統合する
3. `pbt/tests/prop_decoder/response/` を作成し `main.rs` + `status_line.rs` + `body_decoding.rs` + `limits.rs` + `streaming.rs` 等に分割する
