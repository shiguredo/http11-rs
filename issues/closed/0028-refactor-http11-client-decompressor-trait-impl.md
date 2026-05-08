# 0028: examples/http11_client の Decompressor trait 実装の扱いを決める

Created: 2026-05-07
Completed: 2026-05-07
Model: Opus 4.7

## 概要

`examples/http11_client/src/decompressor.rs` には 2 系統のコードが同居しているが、片方が中身の無い空のスケルトンとして放置されている。サンプルとして害があるため「完成させる」か「削除する」かの設計判断を行う。

### 現状の二重構造

(A) **実利用されている一括展開関数** (line 205-261)

- `decompress_body(data: &[u8], encoding: &str) -> Result<Vec<u8>, CompressionError>`
- gzip → `noflate::gzip::decompress` / br → `brotli::BrotliDecompress` / zstd → `zstd::decode_all`
- `main.rs` の `print_response` と新規 `tests/nginx_streaming.rs` の chunked 検証で利用
- `supported_encodings()` も併設

(B) **未使用の Decompressor trait 実装** (line 11-198)

- `GzipDecompressor` / `BrotliDecompressor` / `ZstdDecompressor` の 3 struct
- `shiguredo_http11::compression::Decompressor` トレイトを実装している建前
- ところが `decompress` メソッドの中身は **入力を `self.buffer` に貯めて Continue を返すだけ** で、肝心の展開処理が無い (`_output: &mut [u8]` を完全に無視、`produced: 0` を常に返す)
- struct と inherent impl block には `#[allow(dead_code)]` が付いており、`new()` 呼び出し箇所がどこにも無い → struct 自体が dead code

## 根拠

### 問題 1: 「お手本」として誤解を招く

CLAUDE.md「サンプルは **お手本** なので性能と堅牢性を両立させること」に対して、空のトレイト実装は:

- 形 (シグネチャ) だけ整っていて中身が動かない → 真似する人は「これでストリーミング展開できる」と誤解する
- `#[allow(dead_code)]` で警告を塞いでいるため、リンタも気付かせてくれない
- `Decompressor` トレイトの正しい使い方を示す参考実装になっていない

### 問題 2: ストリーミング展開のサンプルが存在しない

`shiguredo_http11` 本体は `ResponseDecoder::peek_body_decompressed` を提供しており、`Decompressor` トレイト実装を渡せば chunked / large body をメモリ常駐させずにストリーミング展開できる設計。しかし `examples/` に正しく動く実装サンプルが存在しないため、本ライブラリの主要機能の一つが利用例不在のまま放置されている。

### 問題 3: 一括展開 (`decompress_body`) のメモリ効率が悪い

現在のサンプルは body 全体を `Vec<u8>` で受信してから一括展開する。1 GiB のレスポンスを受信したら 1 GiB をまるごとメモリに持つ。サンプルとしての許容範囲を超える可能性がある (本ライブラリの謳う「Sans I/O + ストリーミング decode」のメリットが消える)。

## 選択肢

### 選択肢 A: trait 実装を完成させる (ストリーミング展開化)

- `GzipDecompressor::decompress` / `BrotliDecompressor::decompress` / `ZstdDecompressor::decompress` の中身に noflate / brotli / zstd の **streaming API** を載せる
- `main.rs` の `http_request` 経路を `ResponseDecoder::peek_body_decompressed` 経由に書き換え、受信と並行して展開バイトを取り出す
- `tests/nginx_streaming.rs` で「1 MiB の gzip 圧縮 body をピーク方式で 8 KiB ずつ展開できる」テストを追加できる

**メリット**:

- `Decompressor` トレイトの利用例として完成形を示せる
- ストリーミング展開によりメモリ効率が向上する
- `peek_body_decompressed` 経路のテストカバレッジが上がる (現在ライブラリ本体のドキュメント例だけ)

**デメリット / 検討事項**:

- noflate / brotli / zstd の各 streaming API の仕様調査と実装コストがそれなりに高い
  - noflate (gzip) の sans-io streaming decoder の `decompress(input, output) -> (consumed, produced)` 形式への適合
  - brotli crate の `BrotliState` を使った streaming decode への組み換え
  - zstd crate の `Decoder` (read-based) を sans-io 形式に被せる方法 (zstd は基本 read trait 前提なのでやや難)
- 各 struct が `buffer` フィールドを持っているが、本来 streaming decoder は内部 state を持つ → struct のフィールド構成自体を見直す必要がある
- エラーハンドリングを `CompressionError` に統一するための型変換が必要

### 選択肢 B: trait 実装スケルトンを削除する (簡易サンプル化)

- `GzipDecompressor` / `BrotliDecompressor` / `ZstdDecompressor` の 3 struct と impl をすべて削除する
- `decompress_body` 関数と `supported_encodings` だけ残す (現状の動作維持)
- ストリーミング展開のサンプルは別 example (例: `examples/http11_client_streaming`) として独立させる、またはスコープ外とする

**メリット**:

- 即時対応可能 (削除するだけ)
- 「お手本」としての誤解を招かなくなる
- 一括展開だけのシンプルなサンプルとして用途が明確になる

**デメリット / 検討事項**:

- `Decompressor` トレイトの利用例が examples/ から完全に消える (ライブラリ機能のサンプル不在)
- 大きな body のメモリ効率問題は未解決のまま

### 選択肢 C: 別 example を新設して両立させる

- `examples/http11_client` は一括展開だけに整理する (選択肢 B と同じ)
- 新規に `examples/http11_streaming_decompress` を作り、`Decompressor` trait 実装 + `peek_body_decompressed` のサンプルを書く
- 各 example の責務を明確化する

**メリット**:

- 用途別にサンプルを分けられる
- 既存 `http11_client` を壊さずストリーミング展開のサンプルを追加できる

**デメリット / 検討事項**:

- example が増えることでメンテ対象が広がる (testcontainers テストも増える可能性)
- 同じことを示す example が 2 つできる、と見られる可能性 (差別化を明示する必要あり)

## なぜ pending か

CLAUDE.md「設計判断が必要で保留中の issue は `issues/pending/` に置く」に従う。本 issue は以下の設計判断を要するため、結論を出す前に方針合意が必要:

1. `examples/http11_client` のスコープを「学習用の最小サンプル」と捉えるか「ストリーミング展開も含む包括的サンプル」と捉えるか
2. 選択肢 A を取る場合の実装コスト (各圧縮形式の streaming API 調査) を許容するか
3. 選択肢 C を取る場合の examples ディレクトリの肥大化を許容するか

## 関連

- 0027 (`examples/http11_client` に testcontainers ベースの integration test を追加する) のレビューで本問題が顕在化した
- 本 issue は 0027 のスコープ外として独立に扱う

## 受け入れ基準 (方針確定後)

- 選択肢 A / B / C のどれを取るかが決定されている
- 選択肢が確定したら本 issue を `issues/` 直下に戻し、実装フェーズに入る
- 実装完了後は `issues/closed/` に移動する

## 解決方法

選択肢 A (trait 実装を完成させ、transport.rs を完全ストリーミング化する) を採用した。

### examples/http11_client/src/decompressor.rs の書き直し

- `GzipDecompressor` を `noflate::gzip::Decoder` のラッパーとして再実装
- `BrotliDecompressor` を `BrotliDecompressStream` + `BrotliState` のラッパーとして再実装
- `ZstdDecompressor` を `zstd::stream::raw::Decoder::run_on_buffers` のラッパーとして再実装
- `AnyDecompressor` enum を新設 (`None` / `Gzip(Box<...>)` / `Brotli(Box<...>)` / `Zstd(Box<...>)`)
  - `BrotliState` が ~2.5 KiB と大きいため variant サイズを抑える目的で `Box` 化
  - `for_encoding(encoding: &str)` で Content-Encoding 文字列から動的生成
  - `Decompressor` トレイトを enum 全体に対し impl
- 一括展開関数 `decompress_body` は削除 (`AnyDecompressor` 経路で代替)

### examples/http11_client/src/transport.rs の書き換え

- `ResponseSession` ヘルパーを新設し、受信ループを整理
- `decode_headers` 後に Content-Encoding を見て `AnyDecompressor::for_encoding` で展開器を決定
- `peek_body()` で raw 圧縮バイトを取得し、外部に持つ `AnyDecompressor` で 8 KiB 単位にストリーミング展開
- `decompress_chunk` / `drain_decompressor` ヘルパーで展開ループを抽象化
- main.rs の `print_response` から `decompress_body` 呼び出しを削除 (受信時点で既に展開済み)

### shiguredo_http11 本体の修正 (副次的)

`peek_body_decompressed` がボディデータ枯渇時に展開器の内部 buffer を drain させない構造的制限が判明した。`noflate::gzip::Decoder` のように feed したバイトを内部 buffer に蓄積する型の `Decompressor` 実装と組み合わせるとボディ末尾でバイトを取りこぼすため、以下の変更を入れた:

- `ResponseDecoder::peek_body_decompressed` および `RequestDecoder::peek_body_decompressed` を修正
- ボディデータが None / 空でも空 input で `decompressor.decompress(&[], output)` を呼ぶ
- `consumed == 0 && produced == 0` のときだけ `Ok(None)` を返す
- 後方互換: `NoCompression` のような状態を持たない実装は `Continue { 0, 0 }` または `Complete { 0, 0 }` を返すため、None 判定で従来挙動と等価

### テスト

`examples/http11_client/tests/nginx_streaming.rs` を更新:

- `chunked_response_decoded_properly`: `decompress_body` 呼び出しを削除し、`response.body_bytes()` が直接展開済みであることを検証
- 新規 `streams_large_gzip_body`: 1 MiB クラスの gzip ボディを transport.rs (= AnyDecompressor 経路) で受信し全 byte 一致を検証
- 新規 `peek_body_decompressed_streams_gzip`: `ResponseDecoder::with_decompressor(GzipDecompressor::new())` 経路で 8 KiB バッファに対し `peek_body_decompressed` を繰り返し呼び、1 MiB ボディが完全展開できることを検証 (ライブラリ側の drain 修正のリグレッションテスト)
