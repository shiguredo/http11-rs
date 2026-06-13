# criterion を用いたベンチマーク基盤を追加する

- Priority: Medium
- Created: 2026-06-12
- Completed: {YYYY-MM-DD}
- Model: Opus 4.7
- Branch: feature/add-criterion-bench-foundation
- Polished: 2026-06-13

## 目的

破壊的変更を積極的に行う方針 (CLAUDE.md) のもとで、API および実装の変更が性能に与える影響を「数字で」判断できる基盤を整える。

具体的には以下を可能にする。

- 主要ホットパス (`RequestHead` / `ResponseHead` のデコード、および field-value (obs-text / quoted-string を含む) のパース) の microbench を継続的に計測する
- 統計処理に基づく回帰検出 (criterion の標準機能)

本 issue ではあくまで microbench までを対象とする。`examples/http11_reverse_proxy` をベースにした E2E スループット計測、CI でのベンチ自動実行は別 issue とする。

## 優先度根拠

Medium とする。

- 現状ベンチが一切無く、性能変化を主観で判断するしかない。回帰検出基盤が無いまま破壊的変更を進めるリスクが大きい
- 一方で、明確に回帰している既知バグは無いため、即時のユーザー影響を伴う High ではない
- pbt / fuzz と同程度の重要度 (堅牢性側に対する「性能側」の対と捉える)

## 現状

- ベンチマーク用のクレート・ディレクトリは存在しない (リポジトリルートに `bench/` も `benches/` も無いことを確認)
- 本体 `Cargo.toml` にも `[[bench]]` セクションは無い
- workspace member は `examples/http11_client` / `examples/http11_reverse_proxy` / `examples/http11_server` / `pbt` のみ。`fuzz` と `examples/http11_server_io_uring` は workspace から exclude されている
- `Makefile` には `test` / `cover` / `pbt-with-cover` / `fuzzing` / `fuzzing-parallel` 等のターゲットはあるが、ベンチ関連は無い
- 過去に性能改善 PR (例: `issues/closed/0035-fix-multipart-find-bytes-quadratic-rescan.md`) はあったが、改善幅を裏付けるベンチ結果はリポジトリに残っていない

## 設計方針

### ディレクトリ構成

`bench/` を `pbt/` と同様に独立した workspace member として新設する。

- 本体ライブラリの `dev-dependencies` を criterion で汚さないため、`benches/` ではなく独立クレートとする
- `publish = false`、`version = "0.0.0"`、`edition.workspace = true` は `pbt/Cargo.toml` に揃える
- ルート `Cargo.toml` の `[workspace] members` に `bench` を追加する (`exclude` にはしない。`cargo build --workspace` 等で巻き込むことを許容)

### ベンチランナー

`criterion = { version = "0.5", features = ["html_reports"] }` を採用する。

- 統計処理・回帰判定・HTML レポートが揃っており、microbench に必要十分
- `divan` 等の比較検討は別 issue とし、本 issue では criterion に統一する

### 初期ターゲット

以下を対象とする (実装本体の修正は本 issue では行わない)。

- `RequestHead` デコード
  - 対象: `src/decoder/head.rs:170` `RequestHead`、`src/decoder/request.rs:59` `RequestDecoder`
  - 入力パターン: 最小ヘッダ / 中規模ヘッダ / chunked を想定した `Transfer-Encoding: chunked` 付き
- `ResponseHead` デコード
  - 対象: `src/decoder/head.rs:361` `ResponseHead`
  - 入力パターン: `200 OK` 最小 / 多ヘッダ / `Content-Length` あり
- field-value パース (obs-text / quoted-string を含む経路)
  - 対象: `src/validate.rs` の `is_valid_field_value` (field-value 全体の許容文字検証) と `parse_quoted_string` (quoted-string 内の obs-text / quoted-pair 経路) を利用する公開ヘッダパース API
  - `is_valid_field_value` / `parse_quoted_string` は `pub(crate)` なため、独立クレート `bench` から直接呼び出すことはできない。代わりに `Content-Type::parse` (`src/content_type.rs`)、`Accept::parse` (`src/accept.rs`) 等、内部でこれらを利用する公開 API をベンチ対象とする
  - obs-text は `char_indices()` ベースで Unicode scalar `U+0080..=U+10FFFF` (surrogate 除く) を opaque として保持する (CLAUDE.md / AGENTS.md)
  - 入力パターン: ASCII のみ / obs-text を含む / quoted-string を含む

### 入力データ

- 入力は RFC 9110 / RFC 9112 に準拠した **固定リテラル** を使う (`include_str!` または `include_bytes!` で `bench/inputs/*.txt` を読み込む)
- 乱数・時刻は使わない (計測の再現性を確保)
- サンプルとして残るため、メッセージは規格に準拠したものとする
- `criterion::black_box` を使い、最適化器による消去を防ぐ

### Makefile

`bench` ターゲットを追加し、`cargo bench -p bench` を呼び出す。

### スコープ外 (別 issue 候補)

- CI でのベンチ自動実行 / 結果保存 / 回帰アラート
- `examples/http11_reverse_proxy` をベースにした E2E スループット計測
- divan 等の代替ランナーとの比較
- 改善対象を特定するための深掘り計測 (本 issue は基盤整備のみ)

## 完了条件

- `bench/` クレートが workspace member として追加され、`cargo bench -p bench` でベンチが実行できる
- 以下の microbench が動作する
  - `RequestHead` デコード (最小ヘッダ / 多ヘッダ / chunked)
  - `ResponseHead` デコード (最小 / 多ヘッダ / Content-Length あり)
  - field-value パース (ASCII / obs-text / quoted-string) (`Content-Type::parse` / `Accept::parse` 等の公開 API を利用)
- `Makefile` に `bench` ターゲットが追加され、上記が走る
- `CHANGES.md` の develop セクションに本対応を追記する (種別は `[ADD]`、`shiguredo-changelog` スキルに従う。例: 「criterion を用いたベンチマーク基盤を追加する」)

## 解決方法

1. `bench/Cargo.toml` を作成する
   - `name = "bench"`、`version = "0.0.0"`、`publish = false`、`edition.workspace = true`、`rust-version.workspace = true`
   - `[dependencies]` に `criterion = { version = "0.5", features = ["html_reports"] }`、`shiguredo_http11.workspace = true`
   - 各 bench を `[[bench]] name = "..." path = "benches/<file>.rs" harness = false` で登録する
     - 例: `[[bench]] name = "decode_request_head" path = "benches/decode_request_head.rs" harness = false`
2. `bench/benches/` を作成し、以下のファイルを追加する
   - `bench/benches/decode_request_head.rs`
   - `bench/benches/decode_response_head.rs`
   - `bench/benches/parse_field_value.rs`
3. `bench/inputs/` を作成し、各ベンチで使う RFC 準拠の固定入力をテキストファイルとして配置する
4. 各 bench は `include_str!` または `include_bytes!` で入力を読み込み、`criterion::black_box` を経由してパースを呼び出す
   - `parse_field_value.rs` では `Content-Type::parse` / `Accept::parse` 等、内部で `validate::is_valid_field_value` / `validate::parse_quoted_string` を利用する公開 API を呼び出す
5. ルート `Cargo.toml` の `[workspace] members` に `bench` を追加する
6. `Makefile` の `.PHONY` に `bench` を追加し、`bench` ターゲットを追加する (`cargo bench -p bench`)
7. `CHANGES.md` の develop セクションに追記する (`shiguredo-changelog` 規約に従う)
