# 0066 refactor 長大テストファイルを分割する

Created: 2026-05-14
Completed: 2026-05-14
Model: deepseek-v4-pro
Branch: feature/fix-split-large-test-files

## 概要

以下のテストファイルが CLAUDE.md:97「テストファイルが長くなった場合はファイル内で `mod` を使って分割すること」に違反する規模になっている:

- `tests/test_decoder.rs` (1907 行)
- `pbt/tests/prop_decoder/response.rs` (1317 行)

両ファイルとも複数の独立したテストグループを含んでいる。

### 対象外

- `pbt/tests/prop_decoder/request.rs` (1001 行) — response.rs との対称性から分割が望ましいが、本 issue では見送る (response.rs の分割を手本として後続 issue で対応)。
- `tests/test_request.rs` (640 行) — 単一責務 (Request 構築・バリデーション) で構成されており分割の必要性が低い。
- `tests/test_decoder.rs` 内の既存 `mod` (3 ブロック) は `test_decoder.rs` から切り出さず、分割後の親モジュールファイル (`main.rs`) 内で `mod` 宣言を維持する。

## 分割案

### tests/test_decoder.rs → tests/test_decoder/ ディレクトリ

```
tests/test_decoder/
  main.rs              # エントリポイント、各サブモジュールの mod 宣言 + use 文を集約
  head.rs              # ステータス行・リクエスト行パース、Host ヘッダー検証、HTTP バージョン検証
  body.rs              # ボディデコード基本 (Content-Length / chunked / close-delimited / None)、BodyKind 判定
  streaming.rs         # feed_unchecked / progress / peek_body / consume_body / take_remaining、Keep-Alive、パイプライン
  direct_buffer.rs     # existing mod direct_buffer_write をファイルに昇格 (mut_buf / advance_buf)
  decode_body.rs       # `test_decode_body.rs` の内容を統合。TE トークン検証、chunk-ext ABNF、HTTP バージョン別 TE 拒否、IPv6 ブラケット検証等
```

`test_decode_body.rs` の統合:
- `tests/test_decode_body.rs` は命名規則違反 (対応する `src/decode_body.rs` が存在しない)。
- 内容がリクエストターゲット検証 / TE トークン検証 / chunk-ext / HTTP バージョン別 TE 拒否にわたるため、`decode_body.rs` として独立サブモジュールに配置する。
- `test_decoder.rs` との重複テスト (`http10_with_transfer_encoding_should_fail` 等) は統合時に一方を削除する。
- 統合後に `tests/test_decode_body.rs` は削除する。

`mod` 宣言の構成 (main.rs):
```rust
mod head;
mod body;
mod streaming;
mod direct_buffer;
mod decode_body;
```

### pbt/tests/prop_decoder/response.rs → prop_decoder/response/ ディレクトリ

```
pbt/tests/prop_decoder/
  main.rs              # 変更なし (mod response;) を維持
  response.rs           # mod status_line; mod body_decoding; mod limits; mod streaming; を宣言
  response/
    status_line.rs      # ステータス行生成 proptest
    body_decoding.rs    # ボディデコード (chunked / CL / close-delimited / None)
    limits.rs           # リミット / エラー / 部分データ
    streaming.rs        # Keep-Alive / パイプライン / close-delimited / mark_eof
```

注: `response.rs` 自体は `mod` 宣言のみのファイルとして残り、テスト関数は各サブモジュールファイルに移動する。`response/main.rs` は使わない (CLAUDE.md:99 に反する)。

## 固有の注意点

### use 文の継承

親ファイル (`main.rs` / `response.rs`) の `use` 文はサブモジュールに継承されない。各サブモジュールファイルで必要な `use shiguredo_http11::...` を明示的に記述する。`use super::*` は使わず、明示的なパス指定を推奨する。

### Cargo.toml

Cargo の自動検出により `tests/test_decoder/main.rs` と `pbt/tests/prop_decoder/main.rs` がテストバイナリとして認識される。`Cargo.toml` の変更は不要。

## 確認手順

```bash
cargo test -p shiguredo_http11 --test test_decoder   # 分割後
cargo test -p pbt --test prop_decoder                  # 分割後
cargo test --workspace                                  # 全テスト pass 確認
```

## CHANGES.md

`## develop` の `### misc` に以下を追記する:

```
- [UPDATE] 長大テストファイルをディレクトリモジュールに分割する (CLAUDE.md:97 準拠)
  - `tests/test_decoder.rs` → `tests/test_decoder/` (main / head / body / streaming / direct_buffer / decode_body)
  - `pbt/tests/prop_decoder/response.rs` → `prop_decoder/response/` (status_line / body_decoding / limits / streaming)
  - `tests/test_decode_body.rs` を `tests/test_decoder/decode_body.rs` として統合し元ファイルを削除する
  - @voluntas
```

## ブランチ名

`feature/change-split-large-test-files`

## 受け入れ基準

- [x] `tests/test_decoder/` ディレクトリモジュールが作成され 6 サブモジュールに分割されている
- [x] `pbt/tests/prop_decoder/response/` ディレクトリが作成され 4 サブモジュールに分割されている
- [x] `tests/test_decode_body.rs` が `tests/test_decoder/decode_body.rs` として統合され元ファイルが削除されている
- [x] `test_decoder.rs` と `test_decode_body.rs` の重複テスト (`test_request_http10_transfer_encoding_error` vs `http10_with_transfer_encoding_should_fail`) が解消されている
- [x] `cargo test --workspace` が pass
- [x] 分割前後でテスト数が変化していない (1 件の重複除去のみ)
- [x] `CHANGES.md` にエントリが追記されている

## 解決方法

### tests/test_decoder.rs の分割

`tests/test_decoder.rs` (1907 行、`mod direct_buffer_write` を含む 115 テスト) と `tests/test_decode_body.rs` (1001 行、48 テスト) を `tests/test_decoder/` ディレクトリモジュールに分割した:

- `main.rs` (mod 宣言のみのエントリポイント)
- `head.rs` (32 テスト、`mod http_head_content_length` 11 含む)
- `body.rs` (36 テスト、`mod peek_body_decompressed` 4 含む。重複テスト 1 件削除)
- `streaming.rs` (20 テスト)
- `direct_buffer.rs` (26 テスト、旧 `mod direct_buffer_write` をファイルに昇格)
- `decode_body.rs` (48 テスト、旧 `tests/test_decode_body.rs` を統合)

合計 162 テスト pass (重複削除により 163 → 162)。

### pbt/tests/prop_decoder/response.rs の分割

`pbt/tests/prop_decoder/response.rs` (1317 行、59 proptest) を分割した:

- `response.rs` (mod 宣言のみ、4 行)
- `response/status_line.rs` (16 proptest)
- `response/body_decoding.rs` (15 proptest)
- `response/limits.rs` (9 proptest)
- `response/streaming.rs` (19 proptest)

合計 59 proptest pass。`response/main.rs` は使わず親ファイル + サブディレクトリ構成 (CLAUDE.md:99 準拠)。

### 重複テスト解消

issue で例示された重複テスト `test_request_http10_transfer_encoding_error` (body.rs、簡易版) と `http10_with_transfer_encoding_should_fail` (decode_body.rs、エラーメッセージ検証付き) のうち、エラーメッセージまで検証する後者を残し前者を削除した。

### use 文の継承

親ファイル (`main.rs` / `response.rs`) の `use` 文はサブモジュールに継承されないため、各サブモジュールで `use shiguredo_http11::...` を明示的に記述する形に書き換えた。pbt 側のヘルパー (`body` / `status_code` / `reason_phrase`) は `pub(crate)` 関数のため `use crate::{...}` でアクセスする。

### CHANGES.md

`## develop` の `### misc` に `[UPDATE]` エントリを追記した。

## ブランチ名について

issue 本文は `feature/change-split-large-test-files` を指示していたが、本リファクタリングは公開 API に影響しない (テストファイル分割のみ)。ユーザー判断によりリファクタリングは `[UPDATE]` 種別で扱うため、ブランチも `feature/fix-split-large-test-files` で進めた。CHANGES.md 種別は `[UPDATE]` で issue 指示と一致。
