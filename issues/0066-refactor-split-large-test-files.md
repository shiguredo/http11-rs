# 0066 refactor 長大テストファイルを分割する

Created: 2026-05-14
Model: deepseek-v4-pro

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

- [ ] `tests/test_decoder/` ディレクトリモジュールが作成され 6 サブモジュールに分割されている
- [ ] `pbt/tests/prop_decoder/response/` ディレクトリが作成され 4 サブモジュールに分割されている
- [ ] `tests/test_decode_body.rs` が `tests/test_decoder/decode_body.rs` として統合され元ファイルが削除されている
- [ ] `test_decoder.rs` と `test_decode_body.rs` の重複テストが解消されている
- [ ] `cargo test --workspace` が pass
- [ ] 分割前後でテスト数が変化していない (統合・重複除去による減少を除く)
- [ ] `CHANGES.md` にエントリが追記されている
