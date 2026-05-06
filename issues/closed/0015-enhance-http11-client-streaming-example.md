# 0015: examples/http11_client をストリーミング API のお手本に書き換える

Created: 2026-05-05
Completed: 2026-05-06
Model: Opus 4.7 / deepseek-v4-pro

## 概要

`examples/http11_client/src/main.rs` を `decode()` 一括 API から
ストリーミング API (`decode_headers()` + `peek_body()` / `consume_body()` /
`progress()`) ベースに書き換え、以下を実装する。

- レスポンスヘッダー受信完了時刻 (TTFB)
- 最初に body バイトを観測した時刻 (first-body-byte)
- レスポンス受信完了時刻 (total)

をそれぞれ `Instant` で記録し、`tracing::info!` で出力する。

`BodyKind::Chunked` / `BodyKind::ContentLength(_)` / `BodyKind::CloseDelimited`
すべてで動作するループを書く。

## 根拠

### 動機 1: ストリーミング API のお手本が存在しない

現状の `examples/http11_client` は `decode()` 一括 API しか使っていない。
`examples/http11_reverse_proxy` はストリーミング API を使っているが、
proxy ロジックに埋もれていてお手本としては読みづらい。CLAUDE.md は
「サンプルはお手本」を要求しているが、ストリーミング API の独立したお手本が
リポジトリ内に不在である。

### 動機 2: first-body 計測ユースケースの実証

「Transfer-Encoding: chunked のレスポンスから最初に body バイトを観測した
時刻を記録する」というユースケースをコードで示す。`https://www.google.com/`
は chunked を返すため再現環境として使える。

### 動機 3: issue 0014 の必要性を実コードで判定する

issue 0014 は `BodyProgress` を `Continue` / `Complete` の 2 値から
`Advanced` / `NeedData` / `Complete` の 3 値に細分化する破壊的変更を提案
している。本 issue (0015) のお手本を **現状の API のまま** 書いてみることで、

- 自然に書けるなら 0014 は overengineering なのでクローズ候補
- `remaining_before` 比較などのハックを要求されるなら 0014 の正当化材料

として 0014 の判断材料にする。0014 が pending やクローズに動くか実装される
かは、本 issue の結果を見てから決める。

### 動機 4: `src/decoder/mod.rs` の doc サンプルを修正できる

`src/decoder/mod.rs:23-41` の doc サンプルは「Continue の場合は追加データが
必要 (実際の使用ではネットワーク I/O が必要) `break;`」と無条件 break
する壊れたサンプルになっている。実コードのお手本ができれば、doc は最小限の
動作例にし、詳細は `examples/http11_client` を指せばよくなる。

## 対象ファイルと変更点

### `examples/http11_client/src/main.rs`

`http_request()` / `https_request()` の本体ループを以下の構造に書き換える:

```rust
let connect_at = Instant::now();
// (TLS handshake / TCP connect を含む)

// リクエスト送信後
let request_sent_at = Instant::now();

let mut decoder = ResponseDecoder::new();
let mut head: Option<ResponseHead> = None;
let mut body_kind: Option<BodyKind> = None;
let mut body = Vec::new();
let mut headers_at: Option<Instant> = None;
let mut first_body_at: Option<Instant> = None;

'outer: loop {
    // I/O: バッファ確保 → 読み込み → advance
    let want = decoder.available_buf().min(READ_CHUNK);
    let buf = decoder.mut_buf(want)?;
    let n = stream.read(buf)?;
    if n == 0 {
        decoder.advance_buf(0);
        decoder.mark_eof();
    } else {
        decoder.advance_buf(n);
    }

    // ヘッダー (まだの場合)
    if head.is_none() {
        if let Some((h, k)) = decoder.decode_headers()? {
            headers_at = Some(Instant::now());
            head = Some(h);
            body_kind = Some(k);
        } else if n == 0 {
            return Err("Connection closed before headers complete".into());
        } else {
            continue;
        }
    }

    // ボディ (現状 API でストリーミング読み)
    match body_kind.as_ref().unwrap() {
        BodyKind::None | BodyKind::Tunnel => break 'outer,
        _ => {}
    }
    loop {
        if let Some(data) = decoder.peek_body() {
            if first_body_at.is_none() {
                first_body_at = Some(Instant::now());
            }
            body.extend_from_slice(data);
            let len = data.len();
            match decoder.consume_body(len)? {
                BodyProgress::Complete { .. } => break 'outer,
                BodyProgress::Continue => continue,
            }
        }
        // peek_body() == None: progress() を呼んで状態遷移を試みる
        // 課題: progress() の戻り値だけでは「前進した」「データ不足」を
        //       区別できないため、`remaining()` の前後比較が必要
        let remaining_before = decoder.remaining().len();
        match decoder.progress()? {
            BodyProgress::Complete { .. } => break 'outer,
            BodyProgress::Continue => {
                if decoder.remaining().len() == remaining_before {
                    // 前進していない = ネットワーク I/O 待ち
                    break;
                }
                // 前進している = ループ続行
            }
        }
    }

    if n == 0 {
        // EOF だがまだ完成していない (close-delimited 以外なら異常)
        if matches!(body_kind, Some(BodyKind::CloseDelimited)) {
            // mark_eof() 済みなので次のループで progress() が Complete を返すはず
            continue;
        }
        return Err("Connection closed before response complete".into());
    }
}

let complete_at = Instant::now();
info!(
    ttfb_ms = headers_at.unwrap().duration_since(request_sent_at).as_millis() as u64,
    first_body_ms = first_body_at.map(|t| t.duration_since(request_sent_at).as_millis() as u64),
    total_ms = complete_at.duration_since(request_sent_at).as_millis() as u64,
    "Timing"
);
```

上記コメントの **「課題: progress() の戻り値だけでは...」** が 0014 が
解決しようとしている設計上の問題そのもの。お手本の中にこのコメントが残るのは
望ましくないため、本 issue 完了後に 0014 の判断材料とする。

### `Cargo.toml` (examples/http11_client)

`std::time::Instant` は std にあるので追加依存はなし。`tracing` はすでに
依存にある。

### サンプル動作確認のための README 整備

CLAUDE.md は「ドキュメントは別に書いている」とあるが、`examples/http11_client`
の `Cargo.toml` 冒頭コメントで使用法を示す程度は許容範囲。新規 README は
作らない。

### `src/decoder/mod.rs` の doc サンプル

本 issue では **触らない**。0014 の方針 (3 値化するか現状維持か) が決まった
後に、doc サンプルも実コードに揃える形で修正する。

## ブランチ

`feature/add-http11-client-streaming-example` (機能追加なので `add-`
接頭辞)。issue 番号はブランチ名に含めない。

## 検証

1. `cargo fmt --check`
2. `cargo clippy --workspace -- -D warnings`
3. `cargo build -p http11_client`
4. 実機で動作確認:
   - `cargo run -p http11_client -- https://www.google.com/`
     - chunked レスポンスで TTFB と first-body が記録されること
     - first-body > TTFB であること (sanity check)
   - `cargo run -p http11_client -- https://example.com/`
     - Content-Length レスポンスで同じく記録されること
   - `cargo run -p http11_client -- http://httpbin.org/get`
     - close-delimited になり得る環境でも破綻しないこと
5. 接続失敗系 (DNS 解決失敗、TLS 失敗) でクラッシュせず Err を返すこと

## 0014 との関係

本 issue 完了後、以下を判断する:

- 上記コードの `remaining().len()` 前後比較ハックが「許容範囲」と感じられる
  か → 0014 はクローズ候補
- ハックが「お手本に残すには気持ち悪い」と感じられる → 0014 を実装する
  正当化材料とする

つまり本 issue は 0014 の意思決定の入力でもある。

## 留意点

- `BodyKind::Tunnel` (CONNECT 2xx) は `http11_client` の典型ユースケースでは
  発生しないが、match 漏れにならないよう `BodyKind::None` と同じ扱いで
  break する
- `BodyKind::CloseDelimited` の終端は `mark_eof()` 経由なので、`stream.read`
  が `Ok(0)` を返した直後に `decoder.mark_eof()` を呼んで次のループで
  `progress()` が `Complete` を返すパターンに揃える
- `decoder.available_buf() == 0` (バッファ満杯) のケースは `max_buffer_size`
  超過なのでエラー返却

## 解決方法

### `examples/http11_client/src/main.rs`

`http_request()` / `https_request()` の本体ループを `decode()` 一括 API から
ストリーミング API に書き換えた。

- `decode_headers()` でヘッダーをデコードし、`ResponseHead` と `BodyKind` を取得
- ボディは `peek_body()` / `consume_body()` / `progress()` のループで受信
- `progress()` の戻り値が `Continue` でもデータ不足なのか状態遷移したのか区別
  できないため、`remaining().len()` の前後比較で判定している
  (この点は issue 0014 の判断材料)
- `Instant` で connect, TTFB, first-body-byte, total の各時刻を記録し
  `tracing::info!` で出力
- 全 `BodyKind` (Chunked / ContentLength / CloseDelimited / None / Tunnel) に対応
- HTTPS の TLS `WouldBlock` は既存のハンドリングを維持

### `examples/http11_server/src/main.rs`

合わせてサーバー側もストリーミング API に書き換えた。

- `while let Some(request) = decoder.decode()?` を `decode_headers()` +
  ストリーミングボディ受信に置き換え
- `StreamingState` 構造体で Keep-Alive 間のデコード状態を管理
- `stream_body()` ヘルパー関数でボディ受信処理を共通化
- `serve_request()` ヘルパー関数でリクエスト処理・レスポンス送信・
  Keep-Alive 判定を共通化
- リクエストでは close-delimited は `BodyKind::None` として扱われるため
  (RFC 9112: リクエストは close-delimited を使わない)、
  `mark_eof()` 不要 (`RequestDecoder` に存在しない)
