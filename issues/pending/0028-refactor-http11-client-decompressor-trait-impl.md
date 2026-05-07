# 0028: examples/http11_client の Decompressor trait 実装の扱いを決める

Created: 2026-05-07
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
