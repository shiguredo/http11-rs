# 0014: BodyProgress を細分化してストリーミング API のループ判定を戻り値で完結させる

Created: 2026-05-05
Completed: 2026-05-05
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
- `examples/http11_client` の `remaining_before` 比較ハックを除去し、
  新 `BodyProgress` 3 値のパターンマッチに切り替える
  (ストリーミング API + first-body タイムスタンプ記録は既存)
- `examples/http11_server` の `remaining_before` 比較ハックを除去し、
  新 `BodyProgress` 3 値のパターンマッチに切り替える
- `examples/http11_reverse_proxy` の `remaining_before` 比較ハックを除去し、
  新 `BodyProgress` 3 値のパターンマッチに切り替える

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

### 課題 3: ストリーミング API のサンプルと examples がお手本になっていない

AGENTS.md は「サンプルはお手本なので性能と堅牢性を両立させること」を要求
しているが、

- `src/decoder/mod.rs:23-41` のストリーミング API サンプルは課題 1 のとおり
  途中で壊れる
- `examples/http11_client` / `examples/http11_server` /
  `examples/http11_reverse_proxy` のすべてが課題 1 の `remaining_before`
  比較ハックに依存しており、新 `BodyProgress` 3 値で除去できる状態にある

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
    /// 現在のバッファでさらに処理を試行できる。
    /// 直後に peek_body() / progress() / consume_body() を続けて呼ぶこと。
    Advanced,
    /// バッファに処理可能なデータがなく、追加の feed() が必要。
    /// 呼び出し側はループを抜けてネットワーク I/O に戻る。
    NeedData,
    /// メッセージボディの読み取りが完了した。chunked の場合はトレーラを含む。
    Complete { trailers: Vec<(String, String)> },
}
```

- `Advanced` は「状態機械が前進した」ではなく「呼び出し側が loop を継続
  すべき」という呼び出し側アクション準拠の命名である。phase が多段遷移
  (ChunkedData → ChunkedDataCrlf → ChunkedSize) した場合でも、最終的に
  処理可能であれば `Advanced`、CRLF 不足で処理不可なら `NeedData` を返す。
- `Continue` を `Advanced` に改名するのは、Rust の `loop` の `continue`
  キーワードとの混同を避けるため
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
        let initial_phase = phase.clone(); // 比較用 (真に NeedData かどうか判定)
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

`BodyChunkedSize`、`BodyChunkedDataCrlf`、`ChunkedTrailer` では、内部
`BodyDecoder::consume_body()` の `len` 引数は参照されず、状態遷移だけが行われる。
これはこれらのフェーズが「メタデータ (チャンクサイズ行、CRLF、トレーラ行) の処理中」
であり、ボディデータの消費とは別の処理であるため。

公開 API では `consume_body(len > 0)` は `len == 0` を拒否するため
(`tests/test_decoder.rs:421-431, :434-443`)、これらのフェーズに到達するのは
常に `progress()` → `BodyDecoder::consume_body(0)` の経路のみである。
正しいループパターン (`peek_body()` が `Some` のときだけ `consume_body` を呼ぶ)
では、これらのフェーズで `peek_body()` は `None` を返すため、
公開 API の `consume_body` が呼ばれることはない。

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
  - クレートレベル doc のストリーミング API サンプルを新 `BodyProgress` 3 値
    に追従させる。
  - 既存の `RequestDecoder` サンプル (Content-Length) と、
    `ResponseDecoder` 用の CloseDelimited サンプルを分けて記載する:

```rust
// === RequestDecoder のストリーミング API サンプル ===
// use shiguredo_http11::{RequestDecoder, BodyKind, BodyProgress};

let mut decoder = RequestDecoder::new();
decoder.feed(b"GET / HTTP/1.1\r\nHost: example.com\r\nContent-Length: 5\r\n\r\nhello").unwrap();

let (head, body_kind) = decoder.decode_headers().unwrap().unwrap();
assert_eq!(head.method, "GET");

let mut body = Vec::new();
match body_kind {
    BodyKind::ContentLength(_) | BodyKind::Chunked => loop {
        // バッファにあるボディデータを消費
        if let Some(data) = decoder.peek_body() {
            body.extend_from_slice(data);
            let len = data.len();
            match decoder.consume_body(len).unwrap() {
                BodyProgress::Complete { .. } => break,
                // NeedData (chunked CRLF 不足) でも loop 先頭に戻って peek_body 再試行。
                // peek_body が None なら progress() に fall through する。
                BodyProgress::Advanced | BodyProgress::NeedData => continue,
            }
        }
        // peek_body() が None → 状態機械を進める
        match decoder.progress().unwrap() {
            BodyProgress::Complete { .. } => break,
            // 状態が進んだ: peek_body 再試行のため loop 先頭へ
            BodyProgress::Advanced => continue,
            // バッファ不足: I/O レイヤーに戻って追加データを得る
            BodyProgress::NeedData => break,
        }
    },
    _ => {} // None / Tunnel
}
assert_eq!(body, b"hello");

// === ResponseDecoder の CloseDelimited サンプル (レスポンス専用) ===
// use shiguredo_http11::{ResponseDecoder, BodyKind, BodyProgress};

let mut decoder = ResponseDecoder::new();
decoder.feed(b"HTTP/1.1 200 OK\r\n\r\nhello world").unwrap();

let (head, body_kind) = decoder.decode_headers().unwrap().unwrap();
assert_eq!(body_kind, BodyKind::CloseDelimited);

// mark_eof() 前に peek_body() でバッファ内の全ボディデータを消費する
let mut body = Vec::new();
while let Some(data) = decoder.peek_body() {
    body.extend_from_slice(data);
    let len = data.len();
    decoder.consume_body(len).unwrap();
}
// I/O レイヤーが接続切断を検知したら mark_eof() を呼ぶ
// mark_eof() 後は peek_body() が None を返す (phase == Complete)
assert_eq!(body, b"hello world");
```

CloseDelimited のサンプルが `ResponseDecoder` 用である理由: `mark_eof()` は
`ResponseDecoder` にのみ存在する (`RequestDecoder` は close-delimited を使わないため)。

- `examples/http11_client/src/main.rs`:
  - 既にストリーミング API + first-body タイムスタンプ記録を実装済み。
    以下の 2 点を変更する (HTTP 受信・HTTPS 受信の各 1 箇所):
    1. `progress()` の `remaining_before` 比較ハックを
       `Advanced` / `NeedData` パターンマッチに置換
    2. `consume_body()` の `Continue => continue` を
       `Advanced | NeedData => continue` に置換
  - **注**: `consume_body()` が `NeedData` を返すのは chunked の最終バイト
    消費後 CRLF 不在時のみ。直後の `peek_body()` は `None` (Crlf フェーズ) を
    返すため、内側ループは `progress()` 分岐に fall through して正しく処理される。
    `Advanced | NeedData` を束ねて `continue` するのは意図的な最適化であり、
    NeedData を個別に扱う必要はない。
  - 変更後パターン:

```rust
// consume_body の match
match decoder.consume_body(len)? {
    BodyProgress::Complete { .. } => break 'outer,
    BodyProgress::Advanced | BodyProgress::NeedData => continue,
}
// progress の match
match decoder.progress()? {
    BodyProgress::Complete { .. } => break 'outer,
    BodyProgress::Advanced => continue,
    BodyProgress::NeedData => break,
}
```

- `examples/http11_server/src/main.rs`:
  - 同上。`consume_body()` の `Continue => continue` を
    `Advanced | NeedData => continue` に、`progress()` の `remaining_before`
    ハックを `Advanced`/`NeedData` パターンマッチに置換
    (リクエストボディ受信の `stream_body()` 内 1 箇所)。
  - 変更後パターン・NeedData 到達時の動作は http11_client と同一。

- `examples/http11_reverse_proxy/src/main.rs`:
  - リクエスト方向・レスポンス方向の各 1 箇所、計 2 箇所。
    以下の 2 点を変更する:
    1. `progress()` の `remaining_before` 比較ハックを
       `Advanced` / `NeedData` パターンマッチに置換
    2. `consume_body()` の `Continue => {}` (リクエスト方向) /
       `Continue` アーム (レスポンス方向) を
       `Advanced | NeedData` に置換
  - consume_body の `NeedData` 到達時の動作は http11_client と同一
    (内側ループ継続 → peek_body が None → progress 分岐に fall through)。
  - 変更後パターン:

```rust
// consume_body の match
match decoder.consume_body(len)? {
    BodyProgress::Complete { .. } => break 'outer,
    BodyProgress::Advanced | BodyProgress::NeedData => {} // continue
}
// progress の match
match decoder.progress()? {
    BodyProgress::Complete { .. } => break 'outer,
    BodyProgress::Advanced => continue,
    BodyProgress::NeedData => break,
}
```

### テスト・PBT・fuzz の追従

`Continue` → `Advanced` の機械的置換だけでは済まず、`NeedData` 分岐の追加
が必要な箇所が多い。手作業で全件レビューする。影響範囲 (grep の出現数):

#### 単体テスト

- `tests/test_decode_body.rs` (6 箇所): 各ループから `remaining_before` 比較を
  削除し、`BodyProgress::NeedData` で break する形に書き換える。
  加えて、以下の代表ケースでは戻り値を厳密にアサートする:
  - `incomplete_content_length_body`: `consume_body(50)` は `Advanced`、
    後続の `progress()` は `NeedData` を返すこと
  - `complete_content_length_body`: `consume_body(5)` は `Complete { .. }`
    を返すこと
  - `incomplete_chunked_body`: `consume_body(5)` は
    `Advanced` (CRLF がバッファ内にあるため Crlf→ChunkedSize に遷移)、
    後続の `progress()` は `NeedData` (次のチャンクサイズ行がバッファにない)
    を返すこと

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

プロパティによって、以下の 2 段階で厳密さを選択する:

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

「パニックしないことだけを検証する」は fuzzing の役割であり (AGENTS.md 参照)、
PBT では行わない。どうしても PBT で期待値が決められない場合は、対象を範囲
縮小するか、単体テストに分割する。

#### Fuzz

fuzz ターゲットでは `Continue` を break 条件として使っているパターンが多い。
3 値化後の各ターゲットの方針:

- `fuzz_decoder_chunked.rs`: 実コードは `peek_body()` + `consume_body()` ループと
  `progress()` の両方を使用している。
- `fuzz_decoder_request.rs` / `fuzz_decoder_response.rs`: 実コードは
  `while let Some(data) = decoder.peek_body()` の一括消費ループのみ。`progress()`
  は使用していない。`remaining_before` ハックも不在。
- `fuzz_decoder_roundtrip.rs` / `fuzz_decoder_limits.rs`: 同上、
  `peek_body()` + `consume_body()` のみ。

全 fuzz ターゲット共通ルール: `BodyProgress` の全バリアントを網羅し、
どのアームでも panic しないことを確認する。

#### fuzz ターゲットの具体的な変更パターン

**fuzz_decoder_chunked.rs** — peek_body + consume_body:

変更前:
```rust
if let Some(body_data) = decoder.peek_body() {
    decoded_body.extend_from_slice(body_data);
    let len = body_data.len();
    match decoder.consume_body(len) {
        Ok(BodyProgress::Complete { .. }) => return Some(decoded_body),
        Ok(BodyProgress::Continue) => {}
        Err(_) => return None,
    }
}
```

変更後:
```rust
if let Some(body_data) = decoder.peek_body() {
    decoded_body.extend_from_slice(body_data);
    let len = body_data.len();
    match decoder.consume_body(len) {
        Ok(BodyProgress::Complete { .. }) => return Some(decoded_body),
        Ok(BodyProgress::Advanced | BodyProgress::NeedData) => {}
        Err(_) => return None,
    }
}
```

**fuzz_decoder_chunked.rs** — progress:

変更前:
```rust
match decoder.progress() {
    Ok(BodyProgress::Complete { .. }) => return Some(decoded_body),
    Ok(BodyProgress::Continue) => break, // 追加データが必要
    Err(_) => return None,
}
```

変更後:
```rust
match decoder.progress() {
    Ok(BodyProgress::Complete { .. }) => return Some(decoded_body),
    Ok(BodyProgress::Advanced) => {} // loop 継続
    Ok(BodyProgress::NeedData) => break, // 追加データが必要 → feed へ
    Err(_) => return None,
}
```

**fuzz_decoder_chunked.rs** — 最終チェックの progress:

変更前:
```rust
match decoder.progress() {
    Ok(BodyProgress::Complete { .. }) => return Some(decoded_body),
    Ok(BodyProgress::Continue) => return None, // データ不足で不完全
    Err(_) => return None,
}
```

変更後:
```rust
match decoder.progress() {
    Ok(BodyProgress::Complete { .. }) => return Some(decoded_body),
    Ok(BodyProgress::NeedData) => return None, // データ不足で不完全
    Ok(BodyProgress::Advanced) => {} // loop 継続
    Err(_) => return None,
}
```

**fuzz_decoder_request.rs / fuzz_decoder_response.rs** — while let 一括消費:

実コードは `progress()` を使わず `while let Some(data) = decoder.peek_body()`
の一括消費ループのみ。`consume_body` の match アームを変更する:

変更前:
```rust
while let Some(body_data) = decoder.peek_body() {
    let len = body_data.len();
    match decoder.consume_body(len) {
        Ok(BodyProgress::Complete { .. }) => break,
        Ok(BodyProgress::Continue) => {}
        Err(_) => break,
    }
}
```

変更後:
```rust
while let Some(body_data) = decoder.peek_body() {
    let len = body_data.len();
    match decoder.consume_body(len) {
        Ok(BodyProgress::Complete { .. }) => break,
        Ok(BodyProgress::Advanced | BodyProgress::NeedData) => {}
        Err(_) => break,
    }
}
```

**fuzz_decoder_roundtrip.rs / fuzz_decoder_limits.rs**

同上の `while let` 一括消費パターンのみ。`fuzz_decoder_request.rs` と同一の変更。`progress()` は使用されていない。

##### 全 fuzz ターゲット共通の注意点

- fuzz の目的は「任意入力で panic しないこと」なので、`NeedData` / `Advanced` の
  挙動に厳密な期待は持たず、全バリアントをハンドルして panic を防ぐ
- 全バリアントを列挙するのはコンパイラの網羅性チェックに頼るためではなく、
  各バリアントの意図をコードで明示するためである
- `consume_body` の戻り値は `Advanced | NeedData => {}` とまとめることで、
  消費後の継続か中断かの区別を不要にする (fuzz は単に全データを消費するだけ)

### CHANGES.md

`## develop` セクションに以下を追記する:

- `[CHANGE]` `BodyProgress` を `Advanced` / `NeedData` / `Complete` の 3 値
  に細分化し、追加データが必要な状態を戻り値だけで判定できるようにする
  - @voluntas

### misc

- [UPDATE] `decode()` 内部で使われていた非公開 `available_body_len()` を撤去し、
  `peek_body()` ベースに統一する
  - @voluntas
- [UPDATE] `src/decoder/mod.rs` のストリーミング API doc サンプルを新 `BodyProgress`
  3 値に追従させる
  - @voluntas
- [UPDATE] `examples/http11_client` / `examples/http11_server` /
  `examples/http11_reverse_proxy` の `remaining_before` 比較ハックを
  `BodyProgress` 3 値のパターンマッチに置き換える
  - @voluntas
- [UPDATE] `README.md` と `skills/shiguredo-http11/SKILL.md` の `BodyProgress` に
  関する記述を新 enum に追従させる
  - @voluntas

## 実装の順序

変更の依存関係に基づき、以下の順序で実装する。
各ステップ末尾に記載のコマンドで該当ステップの妥当性を確認する。
`cargo test --workspace` は全テスト・PBT 追従後のステップ 6 以降でのみ実施する。

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

2. `examples/http11_client/src/main.rs`: `remaining_before` 比較ハックを
   3 値パターンマッチに置き換え (HTTP/HTTPS 各 1 箇所)

3. `examples/http11_server/src/main.rs`: `remaining_before` 比較ハックを
   3 値パターンマッチに置き換え (1 箇所)

4. `examples/http11_reverse_proxy/src/main.rs`: `remaining_before` ハックを
   3 値パターンマッチに置き換え (リクエスト方向・レスポンス方向各 1 箇所、計 2 箇所)

5. `cargo check -p http11_client -p http11_server -p http11_reverse_proxy`
   で examples のコンパイルを確認する。

### ステップ 4: テストの追従

1. `tests/test_decode_body.rs`: `remaining_before` ハックを削除し、
   `NeedData => break` に書き換え。全 6 箇所。

2. `tests/test_decoder.rs`: `assert_eq!(result, BodyProgress::Continue)` を
   新しい期待値 (`Advanced` / `NeedData` / `Complete`) に更新。

3. ステップ 4 完了時点で `cargo test --lib` および
   `cargo test --test test_decode_body --test test_decoder` を通過させる。

### ステップ 5: PBT の追従

1. `pbt/tests/prop_decoder/body.rs` (~47 箇所):
   `Continue` → `Advanced` 置換。必要に応じてプロパティを再考。

2. `pbt/tests/prop_decoder/request.rs` (~9 箇所),
   `pbt/tests/prop_decoder/response.rs` (~17 箇所):
   同様。

3. `pbt/tests/prop_request.rs` (~9 箇所): 同様。

4. ステップ 5 完了時点で `cargo test -p pbt` を通過させる。

### ステップ 6: fuzz の追従

1. `fuzz/fuzz_targets/fuzz_decoder_chunked.rs`:
   - `consume_body` の `Continue => {}` → `Advanced | NeedData => {}`
   - `progress` の `Continue => break` → `NeedData => break`, `Advanced => {}`
   - 最終チェックの `Continue => return None` → `NeedData => return None`

2. `fuzz/fuzz_targets/fuzz_decoder_request.rs`, `fuzz/fuzz_targets/fuzz_decoder_response.rs`:
   - `consume_body` の `Continue => {}` → `Advanced | NeedData => {}`
   - (これらの fuzz は `progress()` を使用していない)

3. `fuzz/fuzz_targets/fuzz_decoder_roundtrip.rs`, `fuzz/fuzz_targets/fuzz_decoder_limits.rs`:
   - `consume_body` の `Continue => {}` → `Advanced | NeedData => {}`
   - (これらの fuzz も `progress()` を使用していない)

4. ステップ 6 完了時点で `cargo test --workspace` および
   `cargo clippy --workspace -- -D warnings` を通過させる。

### ステップ 7: CHANGES.md と関連ドキュメントの更新

1. `CHANGES.md`: `## develop` セクションに CHANGES.md 記載のエントリを追記。

2. `README.md`: `BodyProgress` に関する記述を新 enum に追従させる。

3. `skills/shiguredo-http11/SKILL.md`: `BodyProgress` に関する記述を新 enum に追従させる。

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
5. PBT 全件 (`cargo test -p pbt`)
6. fuzz 各 target を短時間 (10〜30 秒) 走らせて緑のままであること
7. `examples/http11_client` を実機で起動し、ログで以下を確認:
    - `cargo run -p http11_client -- https://www.google.com/` (chunked):
      TTFB と first_body_at が記録され、本文が完成形まで取れる
    - `cargo run -p http11_client -- https://example.com/` (Content-Length):
      同上
    - `cargo run -p http11_client -- http://httpbin.org/get` (close-delimited
      になり得る): 同上で破綻しない
8. `examples/http11_server` で `cargo build -p http11_server` が通過すること
9. `examples/http11_reverse_proxy` を起動して同じレスポンスを返せること
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
- `decode()` のループで `peek_body()` → `extend_from_slice(data)` →
  `consume_body(len)` の順序に変更すると、`consume_body` が `BodyTooLarge`
  を返した時点で `decoded_body` にデータが部分的に書き込まれている。
  これは ContentLength/Chunked パスにおける既存の動作 (現コードの
  `request.rs:629-630`, `response.rs:733-734`) と同じであり、新たな問題は
  発生しない。CloseDelimited パスは既存コード同様、`consume_body` 呼出し
  前に max_body_size を事前チェックすること。
