# 0012: ResponseDecoder / RequestDecoder に直接書き込み API (`mut_buf` / `advance_buf` / `available_buf`) を追加する

Created: 2026-05-05
Model: Opus 4.7

## 概要

`ResponseDecoder` および `RequestDecoder` に、内部バッファ末尾の書き込み枠を `&mut [u8]` で取得する `mut_buf(len)` と、実書き込みバイト数を確定する `advance_buf(len)`、書き込み可能な残り容量を問い合わせる `available_buf()` を追加する。これにより OS の `recv()` がデコーダーの内部バッファに直接書き込めるようになり、`feed(&[u8])` 経由で発生していたスタックバッファ → `Vec<u8>` のコピーを排除できる。`available_buf()` は残容量に応じてチャンクサイズを適応させる用途で使う (`bytes::BufMut::remaining_mut()` に対応するヘルパー)。

既存の `feed` / `feed_unchecked` は **既にメモリ上にあるバイト列を投入する** という別役割の API として残す。新 API はバイト列が外部に存在せず「これから書き込む先」を必要とするケース向けで、両者は入力経路が異なる別の最適解である。本変更は純粋追加 (`[ADD]`) となる。

## 根拠

### 現状の問題

`feed(&[u8])` の内部実装は `self.buf.extend_from_slice(data)` であり、外部から渡されたスライスを内部 `Vec<u8>` にコピーする。`examples/http11_client/src/main.rs` の典型パターン:

```rust
let mut buf = [0u8; 8192];
let n = stream.read(&mut buf)?;   // OS → スタックバッファ
decoder.feed(&buf[..n])?;          // スタックバッファ → Vec へコピー (1 回ぶん無駄)
```

read のたびに必ず 1 回の memcpy が発生し、ボディが大きいほど線形に効く。`rustls::Stream::read` も `io_uring` 系も `&mut [u8]` ベースであり、本来中間バッファを置かなくてよい経路にもかかわらず `feed` で 1 段挟まる。

### 回避手段が存在しない

内部 `buf` を露出する公開 API は次のみで、末尾の書き込み枠を `&mut [u8]` で渡す手段が無い:

- `remaining() -> &[u8]` (読み取り専用)
- `take_remaining() -> Vec<u8>` (所有権ごと取り出し / トンネル切替用)

### サンプルが「お手本」になっていない

`CLAUDE.md` は「サンプルはお手本なので性能と堅牢性を両立させること」と規定するが、現状の examples は無駄なコピーを含む実装を示している。

## 設計

### API

```rust
impl<D: Decompressor> ResponseDecoder<D> {
    /// 内部バッファ末尾に len バイトの書き込み枠を確保し、その可変スライスを返す。
    /// 返るスライスはゼロ初期化済みなので std::io::Read::read 等にそのまま渡せる。
    /// 書き込み後は必ず advance_buf で実書き込みバイト数を通知すること。
    /// remaining().len() + len が max_buffer_size を超える場合は Err(BufferOverflow)。
    pub fn mut_buf(&mut self, len: usize) -> Result<&mut [u8], Error>;

    /// 直前の mut_buf で確保した枠のうち、実際に書き込まれた len バイトを確定する。
    /// 残り (mut_buf で確保した長さ - len) は破棄される。
    /// len = 0 で呼ぶと枠全体を破棄 (EOF や read 失敗時のリセット用)。
    pub fn advance_buf(&mut self, len: usize);

    /// 書き込み可能な残り容量を返す。
    /// max_buffer_size から現在のバッファ長 (確定済みデータ + 未確定 pending) を引いた値。
    /// `mut_buf(decoder.available_buf().min(N))` のようにチャンクサイズを残容量に
    /// 適応させる用途で使う。bytes::BufMut::remaining_mut() に対応する。
    pub fn available_buf(&self) -> usize;
}
```

`RequestDecoder` も完全に同じ署名を持つ。

### 利用例

```rust
let mut decoder = ResponseDecoder::new();
const READ_CHUNK: usize = 8192;

loop {
    // 残容量に応じてチャンクサイズを適応させる
    let want = decoder.available_buf().min(READ_CHUNK);
    if want == 0 {
        return Err("decoder buffer full".into());
    }
    let buf = decoder.mut_buf(want)?;
    let n = stream.read(buf)?;
    if n == 0 {
        decoder.advance_buf(0);
        decoder.mark_eof();
        if let Some(response) = decoder.decode()? {
            return Ok(response);
        }
        return Err("Connection closed before response complete".into());
    }
    decoder.advance_buf(n);
    if let Some(response) = decoder.decode()? {
        return Ok(response);
    }
}
```

スタック上の `[u8; 8192]` が消え、OS の `read` が `Vec` の領域に直接書き込む。
`available_buf()` でバッファ末尾の残容量を見て要求サイズを決めるため、`max_buffer_size` ぎりぎりの状態でも graceful に小さく読める。

### 設計判断

1. **API 形状**: `mut_buf` / `advance_buf` の 2-call protocol。`std::io::Read::read(&mut [u8])` の戻り値をそのまま `advance_buf(n)` に渡せる素直な形。`tokio::io::ReadBuf` の前例あり。クロージャ版・BufMut 風 trait・Drop ガード等の代替案は no_std 整合性または API 表面の観点で劣る。
2. **`mut_buf` の戻り型**: `Result<&mut [u8], Error>`。`feed` が `Result` を返すのと対称にし、`max_buffer_size` 超過を確保時点で呼び出し側に伝える。
3. **メモリ初期化**: `Vec::resize(.., 0)` でゼロ初期化。`unsafe` を使わない。要望は「外部バッファ → 内部バッファのコピー」を消すことであり、これはゼロ初期化方式でも完全に達成される (OS の recv が直接 Vec の領域に書き込む)。8KB のゼロ埋めは数 µs で I/O 待ちに比べ無視可能。CLAUDE.md「性能より堅牢性」と整合。
4. **未確定状態の追跡**: 内部に `pending: usize` フィールドを 1 個追加。`mut_buf` の先頭で前回の未確定領域を `truncate` で破棄してから新規確保。
5. **`advance_buf(len > pending)`**: `debug_assert!` で開発時検出、release は `len.min(pending)` で飽和。
6. **`feed()` の扱い**: 残す。`feed(&[u8])` は「既にメモリ上にあるバイト列を投入する」用途で、`extend_from_slice` 1 回 (1 memcpy / ゼロ初期化なし) が最適解となる。一方 `mut_buf` / `advance_buf` は「OS の recv 等が直接書き込む先を確保する」用途。`feed` を消して `mut_buf` 経由に統一すると、すでに `&[u8]` を持っているケースで resize による不要なゼロ初期化 + memcpy の二段になり非効率。両者は冗長ではなく入力経路の違う別の最適解として共存させる。
7. **既存メソッドの誤用検出**: `feed` / `feed_unchecked` / `decode_headers` / `peek_body` / `consume_body` / `progress` / `decode` / `take_remaining` / `mark_eof` の先頭に `debug_assert!(self.pending == 0)` を入れる。
8. **`available_buf()` の追加**: `bytes::BufMut::remaining_mut()` に倣う容量問い合わせヘルパー。外側から `limits().max_buffer_size - remaining().len()` を毎回計算させるのは decoder の内部不変条件を呼び出し側に漏らしており設計的に望ましくない。本質的価値は「無限ループ回避」ではなく「`max_buffer_size` ぎりぎりの状態でチャンクサイズを残容量に適応させ、graceful に小さく読める」点。`pending` が残った状態で呼ばれた場合は `max_buffer_size - buf.len()` (= 未確定領域を引いた残り) を返す。利用側は通常 `advance_buf` 後に `available_buf` を呼ぶため `pending == 0` の状態で評価される。

## 内部実装の擬似コード

```rust
pub fn mut_buf(&mut self, len: usize) -> Result<&mut [u8], Error> {
    if self.pending > 0 {
        let new_len = self.buf.len() - self.pending;
        self.buf.truncate(new_len);
        self.pending = 0;
    }

    let new_size = self.buf.len() + len;
    if new_size > self.limits.max_buffer_size {
        return Err(Error::BufferOverflow {
            size: new_size,
            limit: self.limits.max_buffer_size,
        });
    }

    let old = self.buf.len();
    self.buf.resize(new_size, 0);
    self.pending = len;
    Ok(&mut self.buf[old..])
}

pub fn advance_buf(&mut self, len: usize) {
    debug_assert!(len <= self.pending, "advance_buf len exceeds pending");
    let len = len.min(self.pending);
    let drop = self.pending - len;
    if drop > 0 {
        let new_len = self.buf.len() - drop;
        self.buf.truncate(new_len);
    }
    self.pending = 0;
}

pub fn available_buf(&self) -> usize {
    self.limits.max_buffer_size.saturating_sub(self.buf.len())
}
```

`buf.len()` には `pending` が含まれるため、未確定領域がある状態で `available_buf` を呼ぶと、その分も差し引いた残容量が返る (= 「次に `mut_buf` を呼ぶ前に確定すべきデータ」を含めて見えている残り)。通常利用 (`advance_buf` 後の呼び出し) では `pending == 0` なので結果は「確定済みデータ長を引いた残り」と一致する。

`reset()` では `self.pending = 0` も初期化する。

## 対象ファイル

- `src/decoder/response.rs`
  - `pending: usize` フィールド追加
  - 全コンストラクタ (`new`, `with_limits`, `with_decompressor`, `with_decompressor_and_limits`) で `pending: 0` 初期化
  - `mut_buf`, `advance_buf`, `available_buf` 追加
  - `reset()` で `pending = 0`
  - 既存メソッドの先頭に `debug_assert!(self.pending == 0)`
- `src/decoder/request.rs`
  - 完全に対称な変更
- `examples/http11_client/src/main.rs`
  - `http_request` / `https_request` の受信ループ (2 箇所) を `mut_buf` / `advance_buf` に書き換え
- `examples/http11_server/src/main.rs`
  - `read → decoder.feed(&buf[..n])` パターン (2 箇所) を新 API に書き換え
- `examples/http11_reverse_proxy/src/main.rs`
  - `read → decoder.feed(&buf[..n])` パターン (4 箇所) を新 API に書き換え
- `examples/http11_server_io_uring/src/main.rs`
  - io_uring の completion で `data` を受け取って `feed` する構造のため、すでにメモリ上にあるバイト列を投入する用途に該当する。`feed` のまま残すことでお手本として「メモリ上にあるバイト列の投入には `feed` を使う」ことを示す。実装着手時に該当箇所を改めて確認し、`feed` 維持が妥当であることを最終判断する
- `examples/http11_server/src/compressor.rs` / `examples/http11_server_io_uring/src/compressor.rs`
  - これらの `feed` 呼び出しは `Compressor` trait の `feed` であり、`ResponseDecoder` / `RequestDecoder` の `feed` とは別物。本 issue の対象外
- `tests/test_decoder.rs`
  - 新 API の単体テスト追加 (境界値・エラーパス)
- `pbt/tests/prop_response_decoder.rs` / `prop_request_decoder.rs`
  - PBT に等価性プロパティ追加
- `CHANGES.md`
  - `## develop` セクションに `[ADD]` エントリ追加

## テスト方針

### 単体テスト (`tests/test_decoder.rs`)

エラーパス・境界値のみ:

1. `mut_buf(0)` で空スライスが返る
2. `advance_buf(0)` で pending 領域が破棄される
3. `advance_buf(n)` で `n < pending` のとき残りが捨てられる
4. `mut_buf(len)` で `buf.len() + len > max_buffer_size` のとき `Err(BufferOverflow)` を返し、バッファ状態は不変
5. `mut_buf` を続けて 2 回呼ぶと前回の pending が破棄される
6. `mut_buf` 後 `advance_buf` せずに `feed` を呼ぶと debug ビルドで panic する
7. `reset()` で `pending = 0` に戻る (再 `mut_buf` 可能)
8. 空デコーダーの `available_buf()` は `max_buffer_size` を返す
9. `feed(data)` 後の `available_buf()` は `max_buffer_size - data.len()`
10. `mut_buf(N)` 後 (`advance_buf` 前) の `available_buf()` は `max_buffer_size - (current.len() + N)`
11. `mut_buf(N)` → `advance_buf(M)` (M ≤ N) 後の `available_buf()` は `max_buffer_size - (current.len() + M)`
12. `reset()` 後の `available_buf()` は `max_buffer_size` に戻る

### PBT (`pbt/tests/prop_response_decoder.rs` / `prop_request_decoder.rs`)

等価性プロパティが中心:

1. **`feed` と `mut_buf` + `advance_buf` の等価性**: 任意の HTTP メッセージバイト列に対し、任意のチャンク分割で `feed` を繰り返した場合と、`mut_buf` / `advance_buf` を繰り返した場合で `decode()` の結果が一致する
2. `mut_buf(len)` の戻りスライス長は常に `len`
3. `advance_buf(n)` 後の `remaining().len()` は (前回の remaining) + n
4. `mut_buf` 後 `advance_buf(0)` を挟むと `remaining()` は `mut_buf` 前と同じ

## 検証

- `cargo build --workspace`
- `cargo test --workspace`
- `cargo llvm-cov` で `mut_buf` / `advance_buf` / `available_buf` のカバレッジ確認
- `examples/http11_client` を実機で動かして HTTPS GET が正常完了すること
- `cargo fmt --all -- --check` / `cargo clippy --workspace --all-targets -- -D warnings`

## 影響範囲

- 純粋追加のため後方互換あり (`[ADD]`)
- `pending: usize` フィールドが追加されることで `Debug` 出力が変化するが軽微
- 既存サンプルは新 API ベースに書き換えるため、サンプルコード自体は変わる
