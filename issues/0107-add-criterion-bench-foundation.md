# criterion を用いたベンチマーク基盤を追加する

- Priority: Medium
- Created: 2026-06-12
- Completed: {YYYY-MM-DD}
- Model: Opus 4.7
- Branch: feature/add-criterion-bench-foundation
- Polished: 2026-06-15

## 目的

破壊的変更を積極的に行う方針 (AGENTS.md) のもとで、API および実装の変更が性能に与える影響を「数字で」判断できる基盤を整える。

具体的には以下を可能にする。

- 主要ホットパス (`RequestHead` / `ResponseHead` のデコード、および decoder 経路で踏まれる field-value 検証 / quoted-string パース) の microbench を継続的に計測する
- baseline 保存・比較による回帰検出 (criterion の `--save-baseline` / `--baseline` 機能)

本 issue ではあくまで microbench 基盤の整備までを対象とする。スコープ外項目はまとめて末尾「スコープ外 (別 issue 候補)」節に集約する。

## 優先度根拠

Medium とする。

- 現状ベンチが一切無く、性能変化を主観で判断するしかない。回帰検出基盤が無いまま破壊的変更を進めるリスクが大きい
- 一方で、明確に回帰している既知バグは無いため、即時のユーザー影響を伴う High ではない
- PBT / fuzz と同程度の重要度 (堅牢性側に対する「性能側」の対と捉える)

## 現状

- ベンチマーク用のクレート・ディレクトリは存在しない (`bench/` / `benches/` ともに無い)
- 本体 `Cargo.toml` に `[[bench]]` セクションは無い
- workspace member は `examples/http11_client` / `examples/http11_reverse_proxy` / `examples/http11_server` / `pbt` のみ。`fuzz` と `examples/http11_server_io_uring` は `exclude`
- `Makefile` には `test` / `cover` / `pbt-with-cover` / `fuzzing` / `fuzzing-parallel` 等のターゲットはあるが、ベンチ関連は無い

## 設計方針

### ディレクトリ構成

`bench/` を `pbt/` と同様に独立した workspace member として新設する。

- 本体ライブラリの `dev-dependencies` を criterion で汚さないため、`benches/` ではなく独立クレートとする
- `publish = false`、`version = "0.0.0"`、`edition.workspace = true` / `rust-version.workspace = true` を `pbt/Cargo.toml` に揃える
- ルート `Cargo.toml` の `[workspace] members` に `bench` を追加する (`exclude` にはしない。`cargo build --workspace` / `cargo check --workspace` で巻き込み、API 破壊時に bench も同時にコンパイルエラーで気付ける状態にすることが目的)

### クレート名

クレート名は `bench` ではなく `http11_bench` とする。Cargo の慣用名 `bench` は将来 `cargo install` 対象として衝突するリスクがあり、workspace 内の他クレート (`shiguredo_http11` / `pbt`) との命名対称性も `http11_bench` の方が高い。ディレクトリ名は短さ優先で `bench/` を維持する (`pbt/` と対称)。

### 依存関係

`bench/Cargo.toml` は次のとおりとする。

- `criterion = { version = "0.8", features = ["html_reports"] }`
  - `0.8` 系を採用する。`0.5` 系を採用しない理由は「`std::hint::black_box` 推奨化等の最新ベストプラクティスに追従できないため」 (0.5 系では `criterion::black_box` が現役で `std::hint::black_box` が未推奨)
- `shiguredo_http11.workspace = true`
- `criterion` および `shiguredo_http11` の配置は `[dev-dependencies]` とする (criterion 公式ガイド / `pbt/Cargo.toml` パターンと一致。`[[bench]]` ターゲットは `cargo bench` 時のみコンパイルされ `dev-dependencies` を解決するため、`[dependencies]` に置くと本番依存になり `cargo build` で無駄にリンクされる)

### ベンチランナー

criterion 0.8 を採用する。

- 統計処理・回帰判定・HTML レポートが揃っており、microbench に必要十分
- baseline は `cargo bench -p http11_bench -- --save-baseline <name>` で保存し、`--baseline <name>` で比較する
- `divan` 等の比較検討は別 issue とし、本 issue では criterion に統一する

### 回帰判定の運用

本 issue 完了時点で次の運用を README ないし `Makefile` のコメントに明文化する (CI 自動化は別 issue)。

- 開発開始時に `make bench-save BASELINE=main` で `main` ブランチ相当の baseline を取得する想定
- 変更後に `make bench-cmp BASELINE=main` で比較し、`Change within noise threshold` 以外で `> +5%` の劣化が出たら手動で原因を確認する
- 5% は criterion の標準的な noise threshold (`-t 0.05`) より大きく、雑音と回帰の境目として実用的

回帰判定の自動アラートは CI 統合 issue で別途扱う。

### 初期ターゲット

以下を対象とする (実装本体の修正は本 issue では行わない)。

- `RequestHead` デコード
  - 対象 API: `src/decoder/request.rs:59` `RequestDecoder::new()` + `feed()` + `decode_headers()`
  - 入力パターン: 最小 (1 ヘッダ) / 中規模 (8 ヘッダ) / chunked (`Transfer-Encoding: chunked` 付き 8 ヘッダ)
- `ResponseHead` デコード
  - 対象 API: `src/decoder/response.rs:53` `ResponseDecoder::new()` + `feed()` + `decode_headers()`
  - 入力パターン: `200 OK` 最小 (1 ヘッダ) / 中規模 (8 ヘッダ) / `Content-Length` 付き 8 ヘッダ
- field-value 検証 / quoted-string パース経路 (`validate::is_valid_field_value` / `validate::parse_quoted_string` を踏むデコード経路)
  - 対象 API: 上記 `RequestDecoder` / `ResponseDecoder` のデコード経路 (内部で `parse_header_line` (`src/decoder/body.rs:745`) 経由で `is_valid_field_value` を呼ぶ、`src/decoder/body.rs:778`)。および `ContentType::parse` / `Accept::parse` (内部で `parse_quoted_string` を呼ぶ。`is_valid_field_value` は呼ばない点に注意)
  - 入力パターン: ASCII のみ / obs-text を含む / quoted-string を含む

### 入力データ

- 入力は **固定リテラル** とし、`bench/inputs/*.bin` を `include_bytes!` で読み込む。`include_str!` ではなく `include_bytes!` にする理由は「改行を CRLF (`\r\n`) で厳密に保持するため。`.txt` をエディタが LF に書き換えると decode エラーになる」
- 乱数・時刻は使わない (計測の再現性を確保)
- `std::hint::black_box` を使い、最適化器による消去を防ぐ (criterion 0.8 では `criterion::black_box` は deprecated)

サンプルとして残るため、各ファイルは RFC 9112 / RFC 9110 に準拠したものとする。最小サンプル (`bench/inputs/request_minimal.bin`) は以下のように 1 件貼って手本とする。

```
GET / HTTP/1.1\r\n
Host: example.com\r\n
\r\n
```

(上記表記はエスケープ表示。実ファイルは生の CRLF を保持する。)

入力ファイル一覧と命名:

- `bench/inputs/request_minimal.bin` (1 ヘッダ)
- `bench/inputs/request_medium.bin` (8 ヘッダ、合計約 256 バイト)
- `bench/inputs/request_chunked.bin` (8 ヘッダ + `Transfer-Encoding: chunked`)
- `bench/inputs/response_minimal.bin` (`200 OK` + 1 ヘッダ)
- `bench/inputs/response_medium.bin` (`200 OK` + 8 ヘッダ、合計約 256 バイト)
- `bench/inputs/response_content_length.bin` (`200 OK` + 8 ヘッダ + `Content-Length` 付き)
- `bench/inputs/field_value_in_request_ascii.bin` (ASCII のみの `Content-Type` を含む完全な HTTP リクエスト。decoder 経路で `is_valid_field_value` を計測)
- `bench/inputs/field_value_in_request_obs_text.bin` (obs-text を含む `Content-Type` を含む完全な HTTP リクエスト。同上)
- `bench/inputs/field_value_in_request_quoted_string.bin` (quoted-string を含む `Content-Type` を含む完全な HTTP リクエスト。同上)
- `bench/inputs/field_value_raw_ascii.bin` (ASCII のみの `Content-Type` ヘッダー値そのもの。`ContentType::parse` 経路で計測)
- `bench/inputs/field_value_raw_obs_text.bin` (obs-text を含む同上)
- `bench/inputs/field_value_raw_quoted_string.bin` (quoted-string を含む同上)

「8 ヘッダ・約 256 バイト」の根拠: 中規模入力は「`examples/http11_reverse_proxy` で扱う典型的な HTTP/1.1 リクエスト・レスポンスの実例 (`Host` / `User-Agent` / `Accept` / `Accept-Encoding` / `Connection` / `Cache-Control` / `Content-Type` / `Content-Length` の 8 種を組み合わせた状態)」を最低ラインとして採用する。AGENTS.md の「サンプルは **お手本**」方針に従い、reverse_proxy お手本での「ありふれた中規模リクエスト」を念頭に置く。

### 一次資料の根拠

入力リテラルが準拠する RFC の節番号は以下のとおり (本 issue の bench は規格準拠の入力を計測対象とするため、入力ファイル冒頭コメントにも該当節番号を記録する)。

- request-line: RFC 9112 Section 3 `request-line = method SP request-target SP HTTP-version`
- status-line / reason-phrase: RFC 9112 Section 4 `status-line = HTTP-version SP status-code SP [ reason-phrase ]` / `reason-phrase = 1*( HTAB / SP / VCHAR / obs-text )`
- field-line: RFC 9112 Section 5 `field-line = field-name ":" OWS field-value OWS`
- chunked-body: RFC 9112 Section 7.1
- obs-text: RFC 9110 Section 5.5 `obs-text = %x80-FF`、SHOULD treat ... as opaque data
- quoted-string: RFC 9110 Section 5.6.4 `quoted-string = DQUOTE *( qdtext / quoted-pair ) DQUOTE`

obs-text の取り扱いは AGENTS.md の規約に従い `char_indices()` ベースで Unicode scalar `U+0080..=U+10FFFF` (surrogate 除く) を opaque として保持する前提に変わりがないことを確認するため、`field_value_in_request_obs_text.bin` および `field_value_raw_obs_text.bin` の入力には UTF-8 でエンコードされた非 ASCII バイト列を含める。

### ベンチ Skeleton

`bench/benches/decode_request_head.rs` の Skeleton (代表例):

```rust
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use shiguredo_http11::decoder::RequestDecoder;
use std::hint::black_box;

const INPUT: &[u8] = include_bytes!("../inputs/request_minimal.bin");

fn bench_decode(c: &mut Criterion) {
    c.bench_function("decode_request_head_minimal", |b| {
        b.iter_batched(
            || RequestDecoder::new(),
            |mut decoder| {
                decoder.feed(black_box(INPUT)).unwrap();
                decoder.decode_headers().unwrap()
            },
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(benches, bench_decode);
criterion_main!(benches);
```

`iter_batched` を使う理由は「`RequestDecoder` が内部状態を持つため、1 イテレーションごとに新しいインスタンスを作る必要があるが、毎回 `iter` で計測するとアロケーションのコストも含めてしまう。`iter_batched` で setup と計測を分離する」。`RequestDecoder::new()` の内部はゼロアロックの構造体初期化のみ (`src/decoder/request.rs` の `RequestDecoder::new` を参照) で setup コストは無視できる範囲だが、念のためクロージャ `|| RequestDecoder::new()` 形にして関数項としての解釈や `Default` 型推論の曖昧さを完全に排除する。

### Makefile

`bench` ターゲットを追加し、`cargo bench -p http11_bench` を呼び出す。あわせて `bench-save` と `bench-cmp` の補助ターゲットを追加し、運用ガイドのコメントを Makefile に書く。

`BASELINE` は make 変数として宣言し (`BASELINE ?= main`)、`make bench-save BASELINE=feature-x` のように上書きを許容する。明示的に空文字 (`make bench-save BASELINE=`) を渡された場合は criterion 側で `--save-baseline ` (空文字) になりエラーとなるため、Makefile レシピ冒頭で `@if [ -z "$(BASELINE)" ]; then echo "BASELINE must be non-empty"; exit 1; fi` ガードを入れる。既存 Makefile には変数渡しの先例が無いため、本 issue が初出となる旨をコメントに残す。

### Integrity test

`cargo test --workspace` では `[[bench]]` ターゲットはコンパイルされない。AGENTS.md「Don't live with broken windows」観点で bench が黙って壊れる期間を作らないため、整合性確認用の `#[test]` を `cargo test --workspace` に巻き込む。

Cargo は package に `src/lib.rs` か `src/main.rs` がないと `tests/` 配下の integration test をコンパイルできない (extern crate としてリンクする先が無いため)。本クレートは `[[bench]]` のみだと `tests/` を置けないので、空に近い `bench/src/lib.rs` を 1 つ用意し、整合性確認用の helper 関数 (例: `fn inputs_dir() -> &'static Path`) を置く。

- `bench/src/lib.rs`: 整合性確認のための薄い helper
- `bench/tests/integrity.rs`: integration test。`bench/inputs/*.bin` のファイル名集合と `bench/README.md` 出典一覧に列挙されたファイル名集合との一致を確認する `#[test]` を 1 本置く (突合キーは「ファイル名」)

これによって `cargo test --workspace` で bench が壊れていること (入力ファイル欠落 / README 記述漏れ) を検知できる。

### 出典管理 (`bench/README.md`)

`bench/README.md` 1 枚に以下を集約する。`bench/inputs/README.md` は作らない (二重管理を避ける)。

- bench クレートの目的・起動方法 (`make bench` / `make bench-save` / `make bench-cmp` の使い方)
- 各入力 `bench/inputs/*.bin` の出典 RFC 節番号 (本 issue「一次資料の根拠」節と同じ内容を 1 行ずつ列挙)
- HTML レポート (`target/criterion/`) の共有手段が現状無いことの注意書き (CI 統合 issue でカバーする旨)

### スコープ外 (別 issue 候補)

- CI でのベンチ自動実行 / 結果保存 / 回帰アラート / HTML レポート (`target/criterion/`) の共有
- `examples/http11_reverse_proxy` をベースにした E2E スループット計測
- divan 等の代替ランナーとの比較
- 改善対象を特定するための深掘り計測 (本 issue は基盤整備のみ)

## 完了条件

- `bench/` クレート (`name = "http11_bench"`) が workspace member として追加され、`cargo bench -p http11_bench` でベンチが実行できる
- 以下の microbench が動作する
  - `RequestHead` デコード (最小 / 中規模 / chunked)
  - `ResponseHead` デコード (最小 / 中規模 / Content-Length あり)
  - field-value 検証 / quoted-string パース経路 (ASCII / obs-text / quoted-string) を `RequestDecoder` / `ResponseDecoder` のデコード経路、および `ContentType::parse` / `Accept::parse` の双方で計測する
- `Makefile` に `bench` / `bench-save` / `bench-cmp` ターゲットが追加され、上記が走る
- `make check` (= `cargo check --workspace`) で `bench` クレートが含まれてコンパイル通過することを確認する (CI スモーク方針)
- `make test` (= `cargo test --workspace`) で bench クレートの integrity test (`bench/inputs/*.bin` のファイル名集合と `bench/README.md` 出典一覧のファイル名集合の一致) が実行され、入力ファイルや宣言の食い違いを検知できる
- `target/criterion/` が `.gitignore` に含まれていることを確認する (含まれていない場合は本 issue で追加する)
- `bench/README.md` を新設し、運用ガイド・出典 RFC 節番号一覧・HTML レポート共有のスコープ外注意を集約する
- `CHANGES.md` の develop セクションの `### misc` に本対応を追記する (種別は `[ADD]`。`shiguredo-changelog` スキルに従う。本件はテスト基盤・計測ハーネスの追加であり、過去の PBT / fuzz 追加と同じく `misc` 配置とする)

## 解決方法

1. `bench/Cargo.toml` を作成する
   - `name = "http11_bench"`、`version = "0.0.0"`、`publish = false`、`edition.workspace = true`、`rust-version.workspace = true`
   - `[dev-dependencies]` に以下を記載する (各依存に用途コメントを併記する。shiguredo-rust 規約)
     - `criterion = { version = "0.8", features = ["html_reports"] }` — microbench ランナー
     - `shiguredo_http11.workspace = true` — 本ライブラリ (workspace.dependencies に既存登録 `shiguredo_http11 = { path = "." }` をそのまま参照)
   - 各 bench を `[[bench]] name = "..." path = "benches/<file>.rs" harness = false` で登録する
     - 例: `[[bench]] name = "decode_request_head" path = "benches/decode_request_head.rs" harness = false`
2. `bench/src/lib.rs` を作成する (空に近い lib。`tests/` を有効化するための受け皿。integrity test 用の薄い helper 関数 `pub fn inputs_dir() -> &'static std::path::Path` 程度を置く)
3. `bench/benches/` を作成し、以下のファイルを追加する
   - `bench/benches/decode_request_head.rs`
   - `bench/benches/decode_response_head.rs`
   - `bench/benches/parse_field_value.rs` (decoder 経路と `ContentType::parse` / `Accept::parse` の両方を 1 つの bench ファイルに `bench_function` で並べる)
4. `bench/inputs/` を作成し、設計方針で列挙した 12 ファイル (request 3 / response 3 / field_value_in_request 3 / field_value_raw 3) を RFC 9112 / RFC 9110 準拠の生バイト列として配置する (CRLF 厳守)。各ファイルの出典 RFC 節番号は次の step 5 で作る `bench/README.md` 側で 1 行ずつ管理する
5. `bench/README.md` を新設し、運用ガイド (`make bench` / `make bench-save` / `make bench-cmp`)、`bench/inputs/*.bin` の出典 RFC 節番号一覧、HTML レポート (`target/criterion/`) の共有手段は本 issue スコープ外である旨、の 3 点を集約する。AGENTS.md 規約 (全角と半角の間の半角スペース、絵文字禁止、日本語) は README にも適用する
6. `bench/tests/integrity.rs` を新設し、`bench/inputs/*.bin` のファイル名集合と `bench/README.md` 出典一覧に列挙されたファイル名集合の一致 (突合キーはファイル名) を確認する `#[test]` を 1 つ置く (`cargo test --workspace` で巻き込まれて壊れた状態を検知する)
7. 各 bench は `include_bytes!` で入力を読み込み、`std::hint::black_box` を経由してパースを呼び出す。`iter_batched` で setup と計測を分離する (Skeleton は設計方針節を参照)
8. ルート `Cargo.toml` の `[workspace] members` に `bench` を追加する
9. `Makefile` の `.PHONY` に `bench` / `bench-save` / `bench-cmp` を追加し、3 つのターゲットを実装する。`BASELINE ?= main` を make 変数として宣言し、`make bench-save BASELINE=<name>` / `make bench-cmp BASELINE=<name>` で上書き可能にする。`BASELINE` 空文字に対するガードを各ターゲットの先頭で実施する
10. `.gitignore` を確認し、必要なら `target/criterion/` を追加する (`target/` 全体が既に ignore されていれば追加不要)
11. `CHANGES.md` の develop セクションの `### misc` に `[ADD] criterion を用いたベンチマーク基盤を追加する` を追記する (`shiguredo-changelog` 規約に従う)
