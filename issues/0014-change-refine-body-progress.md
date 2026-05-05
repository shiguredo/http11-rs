# 0014: BodyProgress を細分化してストリーミング API のループ判定を戻り値で完結させる

Created: 2026-05-05
Model: Opus 4.7

## 概要

`BodyProgress` enum を `Continue` / `Complete { trailers }` の 2 値から
`Advanced` / `NeedData` / `Complete { trailers }` の 3 値に変更し、
`progress()` / `consume_body()` の戻り値だけでストリーミング API のループ
判定を完結させる。

同時に以下を行う:

- 内部の非公開 `available_body_len()` を撤去し、`decode()` を `peek_body()`
  ベースに統一する
- `src/decoder/mod.rs` のストリーミング API doc サンプルを正しいループに
  書き直す
- `examples/http11_client` をストリーミング API + first-body タイムスタンプ
  記録のお手本に書き換える
- `examples/http11_reverse_proxy` の `remaining_before` 比較ハックを除去

破壊的変更。`BodyProgress` を pattern match している全箇所 (テスト・PBT・
fuzz・examples) に追従が必要。

## 根拠

ストリーミング API (`decode_headers()` / `peek_body()` / `consume_body()` /
`progress()`) で **「Transfer-Encoding: chunked のレスポンスから最初に body
バイトを観測した時刻を記録する」** というユースケースを実装しようとしたところ、
以下の設計上の課題が判明した。

### 課題 1: `BodyProgress::Continue` が 2 つの意味を抱えている

`progress()` と `consume_body()` が返す `BodyProgress::Continue` は、

- (A) チャンクサイズ行を消費した / バッファに次のデータが残っている
  = **前進した**
- (B) バッファが尽きた / `BodyChunkedDataCrlf` で 2 バイト揃っていない /
  トレーラ行が完成していない = **追加データが必要**

の両方を同じ値で返している。呼び出し側はループの停止条件を戻り値だけで判定
できず、内部状態を間接的に覗く必要がある。

実証:

- `tests/test_decode_body.rs:122-131` は `decoder.remaining().len()` を
  `progress()` 呼び出しの前後で比較し、変化がなければ break するという
  状態の覗き見ハックで判定している (PBT・他テストにも同パターンが多数)
- `examples/http11_reverse_proxy/src/main.rs:316-321`, `660-682` でも同じ
  ハックが使われている
- `src/decoder/mod.rs:34-39` の doc コメントのサンプルは
  「`Continue` の場合は追加データが必要 (実際の使用ではネットワーク I/O が
  必要) `break;`」と無条件 break する壊れたサンプルになっている (chunked で
  複数チャンクが 1 回の read で来た場合、途中で抜けて完成しない)

### 課題 2: 内部 `available_body_len()` と `peek_body()` の機能重複

`src/decoder/response.rs:645` および `src/decoder/request.rs:547` の
非公開メソッド `available_body_len()` は、`peek_body().map(|s| s.len())
.unwrap_or(0)` と完全に等価である。両者を保つことは整合性の不変条件を将来
にわたって維持するコストを生む。

`decode()` は内部で `available_body_len()` を呼んでいるが、これは
`peek_body()` で完全に代替できる (戻り値の slice をその場でコピーすればよい)。

### 課題 3: ストリーミング API のサンプルがお手本になっていない

CLAUDE.md は「サンプルはお手本なので性能と堅牢性を両立させること」を要求
しているが、

- `src/decoder/mod.rs:23-41` のストリーミング API サンプルは課題 1 のとおり
  途中で壊れる
- `examples/http11_client/src/main.rs` は `decode()` 一括 API しか使って
  おらず、ストリーミング API のお手本が不在
- 「最初に body バイトが取れた時刻を記録する」というよくあるユースケースの
  実装例がリポジトリ内に存在しない

### 課題 4: ループ判定ハックを呼び出し側に強いている

課題 1・3 の帰結として、新規ユーザーがストリーミング API でループを書こう
とすると、

1. doc のサンプルを真似る → 壊れる
2. `decode()` の実装を読みに行く → 非公開 `available_body_len()` を使って
   おり、外から再現できない
3. テストや `http11_reverse_proxy` を読む → `remaining().len()` の前後比較
   ハックが必要だと知る

という導線になる。「設計上難しい」という感覚はこの導線そのものに起因する。

## 設計判断

以下の選択肢のうち、本 issue では選択肢 A を採用する。

### 選択肢 A (採用): `BodyProgress` を 3 値に細分化

```rust
pub enum BodyProgress {
    /// 状態が前進した。直後に peek_body() / progress() / consume_body() を
    /// 続けて呼べば、さらに前進できる可能性がある。
    Advanced,
    /// バッファに処理可能なデータがなく、追加の feed() が必要。
    /// 呼び出し側はループを抜けてネットワーク I/O に戻る。
    NeedData,
    /// メッセージボディの読み取りが完了した。chunked の場合はトレーラを含む。
    Complete { trailers: Vec<(String, String)> },
}
```

- `Continue` を `Advanced` に改名するのは、Rust の `loop` の `continue`
  キーワードとの混同を避け「前進した」という意味を明示するため
- `consume_body()` も `BodyChunkedDataCrlf` で `buf.len() < 2` のときに
  `NeedData` を返す価値があるため、`progress()` だけでなく
  `consume_body()` も新 enum を返すように統一する

### 選択肢 B (不採用): `progress()` だけ別 enum (`ProgressResult`) に分離

`consume_body()` の戻り値は現状維持。enum が 2 種類になり学習コストが上がる
ため不採用。

### 選択肢 C (不採用): `available_body_len()` を公開する

`peek_body()` と機能重複する。Sans-I/O の小さい API 表面という設計指針に
反するため不採用。

### 選択肢 D (不採用): `is_blocked()` 等の query メソッドを追加

戻り値の意味を呼び出し側が別経路で問い合わせる形になり、忘れやすい。
enum で返すほうが Rust 的なため不採用。

### 選択肢 E (不採用): `decode_body_chunk()` 等の高レベル API を新設

早すぎる抽象化。`peek_body()` + 新 `BodyProgress` で十分書けるので、まずは
こちらを安定させてから検討する。

## 対象ファイルと変更点

### 公開 API

- `src/decoder/body.rs`:
  - `BodyProgress::Continue` を削除
  - `BodyProgress::Advanced` / `BodyProgress::NeedData` を追加
  - `BodyProgress::Complete { trailers }` はそのまま
- `BodyProgress` は `lib.rs` で `pub use decoder::BodyProgress` として公開
  re-export されているため、これも破壊的変更

### 内部実装

- `src/decoder/body.rs`:
  - `BodyDecoder::consume_body()` の戻り値判定を新 enum に合わせて分岐
  - 判定ルール (フェーズ × 条件 → 戻り値) は以下の表に従う

```
フェーズ (処理後)           | 条件                                           | 戻り値
-------------------------- | ---------------------------------------------- | ---------------------
Complete                   | (任意)                                         | Complete { trailers }
BodyContentLength          | remaining == 0 (消費で残りゼロ)               | Complete { trailers }
BodyContentLength          | remaining > 0 かつ len > 0 を消費             | Advanced
BodyContentLength          | remaining > 0 かつ len == 0 (progress)        | NeedData
BodyChunkedSize            | phase == BodyChunkedSize (不変, 行未発見)     | NeedData
BodyChunkedSize            | phase == Complete (0-size + 全トレーラ処理完) | Complete { trailers }
BodyChunkedSize            | phase が上のいずれでもない (Data / Trailer)   | Advanced
BodyChunkedData            | remaining > 0 かつ len > 0 を消費             | Advanced
BodyChunkedData            | remaining == 0 かつ CRLF あり                 | Advanced (→BodyChunkedSize)
BodyChunkedData            | remaining == 0 かつ CRLF なし                 | NeedData  (→BodyChunkedDataCrlf)
BodyChunkedData            | remaining > 0 かつ len == 0 (progress)        | NeedData
BodyChunkedDataCrlf        | buf.len() >= 2 (CRLF 消費 → BodyChunkedSize)  | Advanced
BodyChunkedDataCrlf        | buf.len() < 2                                | NeedData
ChunkedTrailer             | phase == Complete (終端空行発見)              | Complete { trailers }
ChunkedTrailer             | advanced == true (トレーラ行を処理した)       | Advanced
ChunkedTrailer             | advanced == false (行未発見)                  | NeedData
BodyCloseDelimited         | len > 0 を消費                                | Advanced
BodyCloseDelimited         | len == 0 (progress)                           | NeedData (mark_eof 待ち)
```

**ポイント**: `BodyChunkedData` で `len > 0` を消費して `remaining == 0` に
なった場合、phase が `BodyChunkedDataCrlf` に自動遷移し、CRLF がバッファに
あればさらに `BodyChunkedSize` に進む。この多段遷移後の最終 phase で戻り値を
判定する。

**判定の核**: 「現在のバッファだけでさらに処理を進められるかどうか」。
進められない場合に `NeedData` を返す。たとえば `BodyChunkedData` で残り
データを消費しきって `BodyChunkedDataCrlf` に遷移した場合、データ消費という
意味では前進しているが、CRLF がバッファに無いためこれ以上の処理は不可能。
追加データが必要であることを呼び出し側に伝えるために `NeedData` を返す。

#### consume_body() の実装指針

決定表をコードに落とし込む際の具体的な分岐パターン:

```rust
// BodyContentLength のパターン
match phase {
    DecodePhase::BodyContentLength { remaining } => {
        // len > 0 のときのみ drain (progress は len=0 で呼ばれる)
        buf.drain(..len);
        *remaining -= len as u64;
        self.body_consumed = ...;
        if *remaining == 0 {
            *phase = DecodePhase::Complete;
            return Ok(BodyProgress::Complete { trailers: Vec::new() });
        }
        // len > 0: データを消費した → Advanced
        // len == 0: 何も消費していない → NeedData
        if len > 0 {
            Ok(BodyProgress::Advanced)
        } else {
            Ok(BodyProgress::NeedData)
        }
    }
}

// BodyCloseDelimited のパターン: 消費したかどうかで判定
match phase {
    DecodePhase::BodyCloseDelimited => {
        if len > 0 {
            buf.drain(..len);
            self.body_consumed = ...;
            // mark_eof() が呼ばれるまで Complete にはならない
            Ok(BodyProgress::Advanced)
        } else {
            // progress(): このフェーズでは何も進まない
            // Complete への遷移は mark_eof() のみ
            Ok(BodyProgress::NeedData)
        }
    }
}

// BodyChunkedSize のパターン: process_chunked_size 後の phase で判定
match phase {
    DecodePhase::BodyChunkedSize => {
        let initial_phase = *phase; // 比較用 (真に NeedData かどうか判定)
        self.process_chunked_size(buf, phase, limits)?;
        match phase {
            DecodePhase::Complete => Ok(BodyProgress::Complete { ... }),
            // 行未発見で phase が変わらなかった = 前進しなかった
            _ if *phase == initial_phase => Ok(BodyProgress::NeedData),
            _ => Ok(BodyProgress::Advanced),
        }
    }
}

// BodyChunkedData のパターン: 多段遷移 (Crlf→ChunkedSize) 後の最終 phase で判定
match phase {
    DecodePhase::BodyChunkedData { remaining } => {
        if len == 0 {
            // progress(): このフェーズでは何も進まない
            return Ok(BodyProgress::NeedData);
        }
        buf.drain(..len);
        *remaining -= len;
        if *remaining == 0 {
            *phase = DecodePhase::BodyChunkedDataCrlf;
            // CRLF がバッファにあれば即座に BodyChunkedSize へ
            if buf.len() >= 2 {
                if buf[..2] != *b"\r\n" { return Err(...); }
                buf.drain(..2);
                *phase = DecodePhase::BodyChunkedSize;
            }
        }
        // 最終 phase で判定:
        // BodyChunkedDataCrlf → CRLF 不足で追加データ必要 → NeedData
        // BodyChunkedSize → process_chunked_size の余地あり → Advanced
        // BodyChunkedData (remaining > 0) → まだ消費可能 → Advanced
        if matches!(*phase, DecodePhase::BodyChunkedDataCrlf) {
            Ok(BodyProgress::NeedData)
        } else {
            Ok(BodyProgress::Advanced)
        }
    }
}

// BodyChunkedDataCrlf / ChunkedTrailer のパターン: len 引数は参照しない
match phase {
    DecodePhase::BodyChunkedDataCrlf => {
        // 注: このフェーズでは len は意味を持たない
        // consumed 段階で BodyChunkedData → BodyChunkedDataCrlf → BodyChunkedSize
        // への遷移は自動で行われるため、明示的にこのフェーズに留まっているのは
        // CRLF 不足のため
        if buf.len() >= 2 {
            if buf[..2] != *b"\r\n" { return Err(...); }
            buf.drain(..2);
            *phase = DecodePhase::BodyChunkedSize;
            Ok(BodyProgress::Advanced)
        } else {
            Ok(BodyProgress::NeedData)
        }
    }
    DecodePhase::ChunkedTrailer => {
        // 注: このフェーズでも len は意味を持たない
        let advanced = self.process_trailers(buf, phase, limits)?;
        match phase {
            DecodePhase::Complete => Ok(BodyProgress::Complete { ... }),
            _ if advanced => Ok(BodyProgress::Advanced),
            _ => Ok(BodyProgress::NeedData),
        }
    }
}
```

**len 引数が無視されるフェーズについて**:

`BodyChunkedSize`、`BodyChunkedDataCrlf`、`ChunkedTrailer` では、`consume_body()` の
`len` 引数は参照されず、状態遷移だけが行われる。これはこれらのフェーズが
「メタデータ (チャンクサイズ行、CRLF、トレーラ行) の処理中」であり、
ボディデータの消費とは別の処理であるため。
呼び出し側が `consume_body(len > 0)` を呼んでも、実際には `progress()` と等価の
動作をする。ただし正しいループパターン (`peek_body()` が `Some` のときだけ
`consume_body` を呼ぶ) では、これらのフェーズで `peek_body()` は `None` を返すため、
`consume_body(len > 0)` が呼ばれることはない。

実装では、この不変条件を `debug_assert_eq!(len, 0)` で検出可能にする:

```rust
DecodePhase::BodyChunkedSize => {
    debug_assert_eq!(len, 0, "BodyChunkedSize では consume_body ではなく progress を使うこと");
    // ...
}
DecodePhase::BodyChunkedDataCrlf => {
    debug_assert_eq!(len, 0, "BodyChunkedDataCrlf では consume_body ではなく progress を使うこと");
    // ...
}
DecodePhase::ChunkedTrailer => {
    debug_assert_eq!(len, 0, "ChunkedTrailer では consume_body ではなく progress を使うこと");
    // ...
}
```

release ビルドでは除去されるため実行時コストはゼロ。

**Content-Length: 0 のエッジケース**:

`BodyContentLength { remaining: 0 }` で `progress()` (内部 `consume_body(0)`)
が呼ばれた場合:
- `len == 0` → drain なし、remaining 変化なし
- `*remaining == 0` → phase を `Complete` に遷移、`Complete { trailers }` を返す

この動作は Content-Length: 0 のレスポンスを正しく完了させるために必要であり、
上記実装パターンで自然に処理される。

- `process_trailers()` のシグネチャを `Result<(), Error>` から
  `Result<bool, Error>` に変更する。`bool` は「1 行以上のトレーラを処理したか」。
  これにより `ChunkedTrailer` フェーズで `Advanced` / `NeedData` の判定が可能になる。
  **`process_chunked_size()` の戻り値型は `Result<(), Error>` のまま変更しない。**
  `process_chunked_size()` 内の `process_trailers()` 呼び出しは戻り値の `bool` を
  `let _ = ...?;` で捨てる。呼び出し元 (`consume_body` の `BodyChunkedSize` 分岐)
  は `process_chunked_size` 後の最終 phase で戻り値を判定するため。

- `src/decoder/request.rs`:
  - 非公開 `available_body_len()` を削除
  - `decode()` のループを以下の形に書き換える:

```rust
BodyKind::ContentLength(_) | BodyKind::Chunked => loop {
    // Step 1: バッファからボディデータを直接消費
    if let Some(data) = self.peek_body() {
        self.decoded_body.extend_from_slice(data);
        let len = data.len();
        self.consume_body(len)?;
        // consume_body の戻り値ではなく phase で完了判定することで
        // 内部の多段遷移 (ChunkedData→Crlf→ChunkedSize) を透過的に扱う
        if matches!(self.phase, DecodePhase::Complete) {
            break;
        }
        continue;
    }

    // Step 2: ボディデータがない → 状態機械を進める
    match self.progress()? {
        BodyProgress::Complete { .. } => break,
        BodyProgress::Advanced => continue,  // 状態が進んだ → peek_body 再試行
        BodyProgress::NeedData => return Ok(None),  // 真にデータ不足
    }
},
```

`consume_body` の戻り値で `Complete` 判定するのではなく `self.phase` で
判定する理由: `consume_body` が内部で phase 遷移と CRLF 消費を自動で行うが、
`BodyProgress::Complete` を返すとは限らない (`BodyChunkedData` から
`BodyChunkedDataCrlf` 経由で `BodyChunkedSize` に遷移した場合、戻り値は
`Advanced`)。最終的に Complete に到達したかは phase を見るのが確実。

- `src/decoder/response.rs`:
  - 非公開 `available_body_len()` を削除
  - `decode()` のループを同上に書き換え。
  - `CloseDelimited` 分岐は既存の `mark_eof()` チェックのまま
    (peek_body + consume_body は Advanced/NeedData を返すが、
     Complete 判定は mark_eof 後の phase チェックで行う)

### doc / サンプル

- `src/decoder/mod.rs`:
  - クレートレベル doc コメントのストリーミング API サンプルを 3 値ループに
    書き直す:

```rust
// ヘッダーをデコード
let (head, body_kind) = decoder.decode_headers().unwrap().unwrap();

// ボディをストリーミングで読み取り
let mut body = Vec::new();
match body_kind {
    BodyKind::ContentLength(_) | BodyKind::Chunked => loop {
        // バッファにあるボディデータを消費
        if let Some(data) = decoder.peek_body() {
            body.extend_from_slice(data);
            let len = data.len();
            match decoder.consume_body(len).unwrap() {
                BodyProgress::Complete { .. } => break,
                BodyProgress::Advanced | BodyProgress::NeedData => continue,
            }
        }
        // peek_body() が None → 状態機械を進める
        match decoder.progress().unwrap() {
            BodyProgress::Complete { .. } => break,
            BodyProgress::Advanced => continue,  // 前進したので loop 先頭へ
            BodyProgress::NeedData => break,     // 追加データが必要 → I/O
        }
    },
    BodyKind::CloseDelimited => {
        // 注: mark_eof() が呼ばれると phase が Complete に遷移し、
        // peek_body() は None を返す。そのため mark_eof() の前に
        // バッファ内の全データを消費しきる必要がある。
        // 下記 while let ループは、各 I/O read 後（advance_buf 後）に
        // 全ボディデータを消費する。mark_eof() 後にバッファが空で
        // あれば peek_body() は None、追加の消費は不要。
        while let Some(data) = decoder.peek_body() {
            body.extend_from_slice(data);
            let len = data.len();
            decoder.consume_body(len).unwrap();
        }
        // mark_eof() は I/O レイヤーが接続切断時に呼ぶ
    },
    _ => {} // None / Tunnel
}
```

- `examples/http11_client/src/main.rs`:
  - `decode()` 一括 API を捨て、ストリーミング API (`decode_headers()` +
    `peek_body()` / `consume_body()` / `progress()`) に切り替える
  - I/O ループとボディデコードループを分離せず、両者を一体で書く (お手本)
  - `decode_headers()` 完了時刻 = TTFB、`peek_body()` が初めて `Some(...)`
    を返した時刻 = first-body-byte として `Instant` で記録し、`tracing::info!`
    で出力
  - `BodyKind::Chunked` / `BodyKind::ContentLength(_)` / `BodyKind::CloseDelimited`
    すべてで動作するループにする
  - 疑似コード:

```rust
let t_start = Instant::now();
let mut response_head: Option<ResponseHead> = None;
let mut response_body_kind: Option<BodyKind> = None;
let mut response_body = Vec::new();
let mut first_body_at: Option<Instant> = None;

'read: loop {
    // I/O: データ読み取り
    let want = decoder.available_buf().min(READ_CHUNK);
    let buf = decoder.mut_buf(want)?;
    let n = stream.read(buf)?;
    if n == 0 {
        decoder.advance_buf(0);
        decoder.mark_eof();  // close-delimited 用
    } else {
        decoder.advance_buf(n);
    }

    // ヘッダー未完了 → decode_headers 試行
    if response_head.is_none() {
        if let Some((head, kind)) = decoder.decode_headers()? {
            info!(ttfb_ms = t_start.elapsed().as_millis(), "TTFB");
            response_head = Some(head);
            response_body_kind = Some(kind);
        }
    }

    // ボディデコード
    if let Some(kind) = response_body_kind {
        match kind {
            BodyKind::ContentLength(_) | BodyKind::Chunked => loop {
                if let Some(data) = decoder.peek_body() {
                    if first_body_at.is_none() {
                        first_body_at = Some(Instant::now());
                    }
                    response_body.extend_from_slice(data);
                    let len = data.len();
                    if matches!(decoder.consume_body(len)?, BodyProgress::Complete { .. }) {
                        break 'read;
                    }
                } else {
                    match decoder.progress()? {
                        BodyProgress::Complete { .. } => break 'read,
                        BodyProgress::Advanced => continue,
                        BodyProgress::NeedData => continue 'read,  // 追加データ必要 → I/O
                    }
                }
            },
            BodyKind::CloseDelimited => {
                // バッファ内の全ボディデータを一括消費 (while let で効率的に)
                while let Some(data) = decoder.peek_body() {
                    if first_body_at.is_none() {
                        first_body_at = Some(Instant::now());
                    }
                    response_body.extend_from_slice(data);
                    decoder.consume_body(data.len())?;
                }
                if !decoder.is_close_delimited() {
                    break 'read;  // mark_eof で Complete になった
                }
                continue 'read;
            },
            _ => break 'read,  // None / Tunnel
        }
    }
}

if let Some(t) = first_body_at {
    info!(first_body_at_ms = t.duration_since(t_start).as_millis(), "First body byte received");
}
```

**close-delimited ループの前提**: `mark_eof()` が `DecodePhase::Complete` に
遷移させると `peek_body()` は `None` を返す。そのため、`mark_eof()` を呼ぶ前に
バッファ内の全ボディデータを消費しきる必要がある。上記サンプルでは、
`advance_buf(n)` の直後に `peek_body()` + `consume_body()` でバッファを空にし、
その後に `is_close_delimited()` で完了判定を行っている。
この順序を守れば、`mark_eof()` 後に未消費データが残ることはない。

注: `matches!(decoder.consume_body(len)?, BodyProgress::Complete { .. })`
の完了判定は、`Content-Length` の最終バイト消費時のみ `Complete` にマッチする。
`Chunked` では `consume_body` が `Complete` を返すことはなく、
完了判定は後続の `progress()` 分岐 (`BodyProgress::Complete`) に委ねられる。
これは `BodyChunkedData` の多段遷移が `BodyChunkedSize` で止まり、
`process_chunked_size` を自動呼出ししないため。
両方の完了経路を透過的に扱うには `decode()` 内部のように `self.phase` を
確認するか、上記サンプルのように `consume_body` + `progress()` の両方で
`Complete` をチェックする。

- `examples/http11_reverse_proxy/src/main.rs`:
  - 2 箇所のループ (リクエスト方向 / レスポンス方向) から `remaining_before`
    比較を除去し、新 `BodyProgress` のパターンマッチに揃える
  - 変更後パターン:

```rust
// Before (破棄)
let remaining_before = decoder.remaining().len();
match decoder.progress()? {
    BodyProgress::Complete { .. } => break 'outer,
    BodyProgress::Continue => {
        if decoder.remaining().len() == remaining_before {
            break; // 内側ループを抜けてデータ読み取り
        }
    }
}

// After
match decoder.progress()? {
    BodyProgress::Complete { .. } => break 'outer,
    BodyProgress::Advanced => continue,                // 内側ループ継続
    BodyProgress::NeedData => break,                   // 内側ループを抜けてデータ読み取り
}
```

### テスト・PBT・fuzz の追従

`Continue` → `Advanced` の機械的置換だけでは済まず、`NeedData` 分岐の追加
が必要な箇所が多い。手作業で全件レビューする。影響範囲 (grep の出現数):

#### 単体テスト

- `tests/test_decode_body.rs` (8 箇所): 各ループから `remaining_before` 比較を
  削除し、`BodyProgress::NeedData` で break する形に書き換える。

  変更前:
  ```rust
  BodyProgress::Continue => {
      if decoder.remaining().len() == remaining_before {
          break;
      }
  }
  ```
  変更後:
  ```rust
  BodyProgress::Advanced => continue,
  BodyProgress::NeedData => break,
  ```

- `tests/test_decoder.rs` (5 箇所): `assert_eq!(result, BodyProgress::Continue)`
  を新しい期待値に更新する。特に `consume_body(0)` が Err を返していた箇所
  (`consume_body(0) is not allowed`) は影響を受けない (公開 API の
  `consume_body` の len=0 ガードは維持する)。

#### PBT

`BodyProgress::Continue` → `BodyProgress::Advanced` の機械的置換だけでは不十分。
プロパティの期待値を状況に応じて厳密化する。

##### 期待値の分類指針

プロパティによって、以下の 3 段階で厳密さを選択する:

1. **最も厳密**: 消費後に必ず特定の値が返ることを検証する
   - 例: `BodyContentLength` で `remaining > 0` かつ `len > 0` を消費 →
     `prop_assert_eq!(result, BodyProgress::Advanced)`
   - 例: `BodyChunkedData` で全データ消費後に CRLF なし →
     `prop_assert_eq!(result, BodyProgress::NeedData)`
   - 対象: `pbt/tests/prop_decoder/body.rs` の消費量が制御可能な PBT

2. **やや緩い**: 特定の値で「ない」ことだけを検証する
   - 例: `BodyChunkedSize` から phase が変わった → Complete ではない →
     `prop_assert!(!matches!(result, BodyProgress::Complete { .. }))`
   - 対象: 多段遷移で最終 phase が不定なケース

3. **最も緩い**: パニックしないことだけを検証する
   - 例: Fuzzing 同様、任意入力で全バリアントが返りうることを許容
   - 対象: `prop_decoder/response.rs` の任意入力ラウンドトリップ

##### 具体的な置換方針

- `pbt/tests/prop_decoder/body.rs` (~47 箇所):
  - `Continue` → `Advanced` に置換
  - `prop_assert!(matches!(result, BodyProgress::Continue))` →
    状況に応じて上記 1〜3 のいずれかに置き換え
  - 特に `BodyContentLength` の消費後や `BodyChunkedData` の消費後は
    厳密な期待値を書けるケースが多い

- `pbt/tests/prop_decoder/request.rs` (~9 箇所),
  `pbt/tests/prop_decoder/response.rs` (~17 箇所):
  - `Continue` → `Advanced` に置換
  - 多くのケースでレベル 3 (パニックしないこと) が適切

- `pbt/tests/prop_request.rs` (~9 箇所):
  - 同上

#### Fuzz

fuzz ターゲットでは `Continue` を break 条件として使っているパターンが多い。
3 値化後の各ターゲットの方針:

- `fuzz_decoder_chunked.rs`: `Ok(BodyProgress::Continue) => break` の
  コメント `// 追加データが必要` がついている箇所 → `NeedData => break`
  に変更。それ以外の `Continue => {}` (何もしない) は `Advanced => {}`
  に置換するが、`NeedData` アームも追加して同様に何もしない (fuzz の
  目的はクラッシュ耐性なので、全バリアントをハンドルすればよい)。

- `fuzz_decoder_request.rs` / `fuzz_decoder_response.rs`: 同様に
  機械的置換 + `NeedData` アーム追加。

- `fuzz_decoder_roundtrip.rs` / `fuzz_decoder_limits.rs`: 同様。
  roundtrip 系は完了判定に `Complete` のみを使っているため影響小。

全 fuzz ターゲット共通ルール: `BodyProgress` の全バリアントを網羅し、
どのアームでも panic しないことを確認する。

#### fuzz ターゲットの具体的な変更パターン

##### fuzz_decoder_chunked.rs の典型パターン

変更前:
```rust
loop {
    match decoder.progress() {
        Ok(BodyProgress::Continue) => {
            // 追加データが必要 → feed に戻る
            if decoder.remaining().is_empty() {
                break;
            }
            // まだバッファにデータがある → loop 継続
        }
        Ok(BodyProgress::Complete { .. }) => break,
        Err(_) => return,
    }
}
```

変更後:
```rust
loop {
    match decoder.progress() {
        Ok(BodyProgress::Advanced) => {
            // 前進した → loop 継続してさらに処理
        }
        Ok(BodyProgress::NeedData) => {
            // 追加データが必要 → feed に戻る
            break;
        }
        Ok(BodyProgress::Complete { .. }) => break,
        Err(_) => return,
    }
}
```

##### fuzz_decoder_request.rs / fuzz_decoder_response.rs の典型パターン

変更前 (remaining_before ハック):
```rust
let remaining_before = decoder.remaining().len();
match decoder.progress()? {
    BodyProgress::Complete { .. } => break,
    BodyProgress::Continue => {
        if decoder.remaining().len() == remaining_before {
            break; // 進んでいない → feed へ
        }
    }
}
```

変更後:
```rust
match decoder.progress()? {
    BodyProgress::Complete { .. } => break,
    BodyProgress::Advanced => {} // continue loop
    BodyProgress::NeedData => break, // feed へ
}
```

##### fuzz_decoder_roundtrip.rs / fuzz_decoder_limits.rs

これらのターゲットは `Complete` 判定のみを使用しており、`Continue` → `Advanced`
の置換と `NeedData` アームの追加で十分。例:

変更後:
```rust
match decoder.progress() {
    Ok(BodyProgress::Complete { .. }) => break,
    Ok(BodyProgress::Advanced) => {} // continue
    Ok(BodyProgress::NeedData) => {} // continue (fuzz では feed 継続)
    Err(_) => break,
}
```

##### 全 fuzz ターゲット共通の注意点

- fuzz の目的は「任意入力で panic しないこと」なので、`NeedData` / `Advanced` の
  挙動に厳密な期待は持たず、全バリアントをハンドルして panic を防ぐ
- `#![no_main]` の fuzz_target では `match` の網羅性チェックが働かないため、
  手作業で全バリアントを列挙する
- 既存の `continue` や `break` の分岐先に影響がないよう、論理の流れを
  `Advanced` → loop 継続、`NeedData` → feed に戻る、に統一する

### CHANGES.md

`## develop` セクションに以下を追記する (順序は CHANGE が先):

- `[CHANGE]` `BodyProgress` を `Advanced` / `NeedData` / `Complete` の 3 値
  に細分化し、追加データが必要な状態を戻り値だけで判定できるようにする
  - @voluntas
- `[CHANGE]` `decode()` 内部で使われていた非公開 `available_body_len()` を
  撤去し、`peek_body()` ベースに統一する
  - @voluntas

doc / サンプル修正は `### misc` には入れず、上記 CHANGE エントリの巻き添え
として扱う。

## 実装の順序

変更の依存関係に基づき、以下の順序で実装する。
ステップ 1-3 ではコンパイル (`cargo check --lib`) の通過を、
ステップ 4 以降では `cargo test --workspace` と
`cargo clippy --workspace -- -D warnings` の通過を確認する。

### ステップ 1: コア enum と `BodyDecoder::consume_body()` の書き換え

1. `src/decoder/body.rs`:
   - `BodyProgress` enum を 3 値 (`Advanced` / `NeedData` / `Complete { trailers }`)
     に変更
   - `process_trailers()` のシグネチャを `Result<bool, Error>` に変更
   - `process_chunked_size()` 内の `process_trailers()` 呼び出しを `let _ = ...?;`
     に修正
   - `consume_body()` の各分岐を決定表・実装指針に従って書き換え
   - `BodyChunkedSize` / `BodyChunkedDataCrlf` / `ChunkedTrailer` に
     `debug_assert_eq!(len, 0)` を追加 (len 無視フェーズの規約違反検出)

2. この時点でコンパイルは通らない (`Continue` を参照している全箇所がエラーになる)
   が、それが意図した動作。ステップ 2 完了後に `cargo check --lib` が通過する。

### ステップ 2: デコーダーの公開 API と内部実装

1. `src/decoder/request.rs`:
   - `available_body_len()` を削除
   - `decode()` のループを `peek_body()` + `phase` チェック方式に書き換え
   - `consume_body()` / `progress()` の戻り値型は自動的に `BodyProgress` に追従

2. `src/decoder/response.rs`:
   - `available_body_len()` を削除
   - `decode()` のループを `peek_body()` + `phase` チェック方式に書き換え
   - `CloseDelimited` 分岐も `peek_body()` に切り替え

3. ステップ 1-2 完了時点で `cargo check --lib` が通過することを確認する。

### ステップ 3: doc / サンプルの書き換え

1. `src/decoder/mod.rs`: クレートレベル doc のストリーミング API サンプルを 3 値
   ループに書き換え

2. `examples/http11_client/src/main.rs`: ストリーミング API + first-body
   タイムスタンプ記録に書き換え

3. `examples/http11_reverse_proxy/src/main.rs`: `remaining_before` ハックを
   3 値パターンマッチに置き換え

4. `cargo check --examples` で examples のコンパイルを確認する。

### ステップ 4: テストの追従

1. `tests/test_decode_body.rs`: `remaining_before` ハックを削除し、
   `NeedData => break` に書き換え。全 8 箇所。

2. `tests/test_decoder.rs`: `assert_eq!(result, BodyProgress::Continue)` を
   新しい期待値 (`Advanced` / `NeedData` / `Complete`) に更新。

3. ステップ 4 完了時点で `cargo test --workspace` (doc test 含む) を通過させる。

### ステップ 5: PBT の追従

1. `pbt/tests/prop_decoder/body.rs` (~47 箇所):
   `Continue` → `Advanced` 置換。必要に応じてプロパティを再考。

2. `pbt/tests/prop_decoder/request.rs` (~9 箇所),
   `pbt/tests/prop_decoder/response.rs` (~17 箇所):
   同様。

3. `pbt/tests/prop_request.rs` (~9 箇所): 同様。

### ステップ 6: fuzz の追従

1. `fuzz_targets/fuzz_decoder_chunked.rs`:
   - `Continue => break` + `// 追加データが必要` → `NeedData => break`
   - `Continue => {}` → `Advanced => {}` (+ `NeedData => {}` アーム追加)

2. `fuzz_targets/fuzz_decoder_request.rs`, `fuzz_decoder_response.rs`:
   - `remaining_before` ハックを `Advanced` / `NeedData` パターンマッチに置換

3. `fuzz_targets/fuzz_decoder_roundtrip.rs`, `fuzz_targets/fuzz_decoder_limits.rs`:
   - `Continue` → `Advanced` 置換 + `NeedData` アーム追加

### ステップ 7: CHANGES.md の更新

`## develop` セクションに CHANGES.md 記載のエントリを追記。

### ステップ 8: 統合検証

検証セクションの全項目を実施。

## ブランチ

`feature/change-refine-body-progress` (後方互換のない変更なので `change-`
接頭辞)。issue 番号はブランチ名に含めない。

## 検証

1. `cargo fmt --check`
2. `cargo clippy --workspace -- -D warnings`
3. `cargo test --workspace` で全テスト緑
4. `cargo llvm-cov` で `consume_body` の各分岐 (`Advanced` / `NeedData` /
   `Complete`) がカバーされていることを確認
5. PBT 全件 (`cargo test -p pbt --test prop_decoder`)
6. fuzz 各 target を短時間 (10〜30 秒) 走らせて緑のままであること
7. `examples/http11_client` を実機で起動し、ログで以下を確認:
   - `cargo run -p http11_client -- https://www.google.com/` (chunked):
     TTFB と first_body_at が記録され、本文が完成形まで取れる
   - `cargo run -p http11_client -- https://example.com/` (Content-Length):
     同上
   - `cargo run -p http11_client -- http://httpbin.org/get` (close-delimited
     になり得る): 同上で破綻しない
8. `examples/http11_reverse_proxy` を起動して同じレスポンスを返せること
   (smoke test)

## 留意点

- `BodyProgress` を pattern match している箇所が多数あるため、ビルド時に
  漏れなく追従できるよう、enum バリアント名を変える (`Continue` → `Advanced`)
  選択は意図的。コンパイラエラーで残漏れを検出させる。
- `consume_body()` の戻り値が `Advanced` か `NeedData` かの境界は微妙な
  ケース (`BodyChunkedData` の最後の 1 バイトを消費した直後など) を含む。
  PBT で chunked の境界条件を強化する余地あり。
- `process_trailers()` の戻り値型が `Result<(), Error>` から
  `Result<bool, Error>` に変わる。`process_chunked_size()` 内の
  `self.process_trailers(...)` 呼び出し箇所も戻り値の受け取り方を修正すること。
- `decode()` の内部ループでは `consume_body` の戻り値ではなく
  `matches!(self.phase, DecodePhase::Complete)` で完了判定する。
  これは `consume_body` が内部で多段遷移した場合に `Complete` ではなく
  `Advanced` を返すことがあるため。呼び出し側が phase を直接読むのは
  decode() 内部実装に限られる (公開 API の利用者は BodyProgress で判定する)。
- `DecodePhase` は `pub(crate)` なので、リクエスト・レスポンスデコーダー
  内部からのみ phase を直接参照できる。この制約により設計のカプセル化は
  保たれる。
