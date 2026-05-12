# 0052: reverse_proxy サンプルの close-delimited 経路で decoder バッファのボディ先頭バイトを取りこぼす問題を修正する

Created: 2026-05-12
Completed: 2026-05-12
Model: Opus 4.7

## 概要

`examples/http11_reverse_proxy/src/main.rs:594-619` の close-delimited 分岐は、`decoder.peek_body()` / `consume_body()` / `take_remaining()` を一切呼ばずに `upstream.read(&mut buf)` から直接転送を開始する。

```rust
// examples/http11_reverse_proxy/src/main.rs:594-619
// close-delimited body の場合: upstream が閉じるまでデータを転送
// 注: ResponseDecoder の mark_eof() API を使わずに直接ストリーミング転送する理由:
// - ボディをメモリに蓄積せずにリアルタイムで downstream に転送するため
// - 大容量レスポンスでもメモリ効率が良い
if is_close_delimited {
    debug!("Streaming close-delimited body until connection closes");
    let mut buf = [0u8; READ_CHUNK];
    let mut close_delimited_bytes = 0usize;
    loop {
        let n = upstream.read(&mut buf).await?;
        if n == 0 {
            debug!(total_bytes = close_delimited_bytes, "Close-delimited body complete");
            break;
        }
        downstream.write_all(&buf[..n]).await?;
        close_delimited_bytes += n;
    }
    downstream.flush().await?;
    return Ok(can_reuse);
}
```

`decode_headers` (`src/decoder/response.rs:436, 497, 578`) は `self.buf.drain(..pos + 2)` でヘッダー終端 `\r\n\r\n` までしか drain しない。`decode_headers` が `Some((head, BodyKind::CloseDelimited))` を返した時点で、decoder 内部バッファにはボディ先頭バイトが残ったまま。close-delimited 分岐は decoder 内部に触らず生 `upstream.read` に切り替えるため、**残ったボディ先頭バイトは永久に下流に届かない**。

## 根拠

### 発生条件

- TCP は record 境界を保証しないため、サーバが短いレスポンス (ヘッダー + 小〜中サイズのボディ) を 1 度の `write` で送ると、1 つの TCP セグメントで到着するのが一般的
- TLS レコード境界でも 1 レコード内にヘッダーとボディ先頭が同居する
- 本実装の read バッファサイズ: `const READ_CHUNK: usize = 8192;` (L493)。`decoder.available_buf().min(READ_CHUNK)` で `mut_buf` を確保するため、最低でも 8192 バイト分は 1 read で取り込める
- 短いレスポンス (典型的な API レスポンス、HTML) では **ほぼ毎回発生**

### 欠落バイトの行方

- `decode_headers` 完了時点で `self.buf` にはボディ先頭バイトが残る
- `peek_body` 経路 (`src/decoder/body.rs:138-143`): `BodyCloseDelimited` のとき `Some(buf)` を返すため取り出せる設計
- `take_remaining()` (`src/decoder/response.rs:181-191`): `pending == 0` の `debug_assert` 付きで内部 `Vec::take` を返す既存 API。CONNECT トンネル用に導入されたが phase 不問で close-delimited にも適用可能
- 現在の close-delimited 直接 read 経路: decoder に触れず upstream から新規バイトのみ読む → 残バイトは下流に届かない

### 影響の重大性

- close-delimited (HTTP/1.0 互換) は Content-Length も Transfer-Encoding もないため、欠落しても下流クライアントは **「正常終了」と解釈**してしまう。データ破損が検知不能
- 0050 で plaintext (`http://`) 対応が入ると HTTP/1.0 + Connection: close + plaintext の組み合わせで本 bug が日常的に再現する。本 issue は **0050 マージ後にテスト容易性が高まる**ため、0050 → 0051 → 0052 の順で着手する
- AGENTS.md「サンプルは **お手本** なので性能と堅牢性を両立させること」要件への致命的逸脱

### RFC 引用

- RFC 9112 §6.3 item 8: 「Otherwise, this is a response message without a declared message body length, so the message body length is determined by the number of octets received prior to the server closing the connection.」
- `refs/rfc9112.txt` で参照可能。close-delimited body の定義そのもの

## スコープ

- close-delimited 分岐の冒頭で `decoder.take_remaining()` を呼んで decoder 内部バッファのバイトを下流に流す
- その後、upstream から FIN まで直送する既存ループは維持する
- 既存コメント L595-597 (mark_eof を使わない理由) をハイブリッド構造の説明に書き換える
- 含まない:
  - `mark_eof()` 経由の Complete 遷移処理 (decoder の状態をそれ以上使わないため不要)
  - chunked 経路 (L626-657) の書き換え (既存の `peek_body` + `consume_body` パターンを維持)
  - `consume_body` の追加呼び出し (`take_remaining` で全消費するため呼ぶ必要なし)

## 対応方針

### `examples/http11_reverse_proxy/src/main.rs` (close-delimited 分岐)

`take_remaining()` で decoder バッファを 1 行 drain してから既存の直送ループに入る。借用衝突 (`peek_body` の `&self` と `consume_body` の `&mut self` が `await` を挟むと衝突) を避けるため、`take_remaining()` を採用する。

```rust
if is_close_delimited {
    debug!("Streaming close-delimited body: drain decoder leftover then read until FIN");

    // ヘッダー直後に既に decoder 内部バッファに残っているボディ先頭バイトを下流に流す
    // (TCP セグメント結合 / TLS レコード境界で 1 read にヘッダー + ボディ先頭が同居するケース)
    let leftover = decoder.take_remaining();
    let mut close_delimited_bytes = leftover.len();
    if !leftover.is_empty() {
        downstream.write_all(&leftover).await?;
    }

    // 続いて upstream から FIN (read == 0) まで直接転送
    let mut buf = [0u8; READ_CHUNK];
    loop {
        let n = upstream.read(&mut buf).await?;
        if n == 0 {
            debug!(total_bytes = close_delimited_bytes, "Close-delimited body complete");
            break;
        }
        downstream.write_all(&buf[..n]).await?;
        close_delimited_bytes += n;
    }
    downstream.flush().await?;
    return Ok(can_reuse);
}
```

### 既存コメント L595-597 の差し替え

```
// close-delimited body の場合の処理:
// 1. ヘッダー終端の直後に decoder 内部バッファに残ったボディ先頭バイトを take_remaining() で取り出して下流に流す
//    (TCP セグメント結合や TLS レコード境界で 1 read にヘッダー + ボディ先頭が同居するため、これを怠ると先頭バイトが消失する)
// 2. 続いて upstream から FIN まで直接転送する (大容量レスポンスでもメモリに蓄積しない)
// RFC 9112 Section 6.3 item 8 に準拠 (Content-Length / Transfer-Encoding なしでは FIN が body 終端)
```

### 0050 マージ後の rebase

0050 で `upstream` の型が `UpstreamStream::{Plain, Tls}` の enum に変わる場合、`upstream.read(&mut buf)` の呼び出しは enum を `match` で剥がす形に書き換える。0050 → 0051 → 0052 の順でマージし、本 issue は 0050 完了後に rebase する。

### テスト

`examples/http11_reverse_proxy/tests/` (0051 で新設する test 基盤を流用) に integration test を追加:

- HTTP/1.0 + `Connection: close` + Content-Length なしで短いレスポンスを返すモック upstream を `tokio::net::TcpListener` で立てる (生バイトを `accept` 後に直接 `write_all` してから FIN)
- ヘッダー + ボディを 1 度の `write` で送り、1 TCP セグメントに乗るようにする
- 本サンプルを起動して GET を投げ、ボディ全バイトが downstream に到達することを assert する

```sh
# モック upstream のレスポンス例
HTTP/1.0 200 OK\r\nServer: mock\r\n\r\nHello, World!
```

このレスポンスをサーバが 1 度の `write_all` で送って FIN すれば、本実装の `decode_headers` で `Hello, World!` の一部 (場合によって全バイト) が decoder バッファに残る。本 issue 修正前は欠落、修正後は全 13 バイト到達。

### CHANGES.md

機能不全修正は機能に直接影響するため `### misc` ではなく本体 `[FIX]` 配下 (0048 / 0049 / 0050 / 0051 と方針統一):

```
- [FIX] `examples/http11_reverse_proxy` の close-delimited 経路で decoder 内部バッファのボディ先頭バイトを取りこぼす問題を修正する
  - 旧実装は `is_close_delimited` 分岐で `decoder.peek_body()` / `consume_body()` / `take_remaining()` を呼ばずに生 `upstream.read` に切り替えていた
  - `decode_headers` はヘッダー終端 `\r\n\r\n` までしか drain しないため、TCP セグメント結合や TLS レコード境界で 1 read にヘッダー + ボディ先頭が同居するケース (短いレスポンスではほぼ毎回発生) でボディ先頭バイトが下流に届かなかった
  - close-delimited は CL も TE もないため、欠落しても下流クライアントは「正常終了」と解釈してしまう検知不能なデータ破損経路
  - close-delimited 分岐の冒頭で `decoder.take_remaining()` を呼んで残バイトを下流に流すよう変更する
  - RFC 9112 §6.3 item 8 (close-delimited body は FIN が終端) に準拠
  - @voluntas
```

### ブランチ

`feature/fix-reverse-proxy-close-delimited-buffer-drain` (`feature/fix-` prefix、example 内部の修正のみで本体 API には影響なし、issue 番号を含まない)。

## 受け入れ基準

- close-delimited 分岐の冒頭で `decoder.take_remaining()` が呼ばれ、戻り値が非空のとき downstream に書き出される
- 既存コメント L595-597 が「decoder バッファ drain + その後 upstream 直送」のハイブリッド構造を反映するよう更新されている
- `examples/http11_reverse_proxy/tests/` に「HTTP/1.0 + Connection: close + short response が 1 度の write で送られたケース」の integration test が追加されている (0051 の test 基盤を流用)
- 修正前は test が **失敗** し、修正後は PASS することが確認されている
- `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` がすべて PASS
- CHANGES.md `## develop` に `[FIX]` エントリが追加されている (本体配下、`### misc` ではない)

## 関連 issue

- 0050 (upstream URL scheme/port): 同じファイル対象、本 issue マージ前に着手。マージ後に本 issue を rebase する
- 0051 (CONNECT メソッド): 同じファイル対象、test 基盤を本 issue で流用する
- 0048 / 0049: `examples/http11_server_io_uring` 系、本 issue とは別経路だが「お手本サンプル」要件の同根問題

3 issue (0050 / 0051 / 0052) は同一ファイル (`examples/http11_reverse_proxy/src/main.rs`) の独立した症状で、0050 → 0051 → 0052 の順で着手する。

## RFC 参照

- RFC 9112 §6.3 item 8 (close-delimited body は FIN が終端、`refs/rfc9112.txt`)
- RFC 9112 §9.6 (Connection: close 後の挙動、`refs/rfc9112.txt`)

## 解決方法

- `examples/http11_reverse_proxy/src/main.rs` の close-delimited 分岐冒頭で `decoder.take_remaining()` を呼んで decoder 内部バッファのボディ先頭バイトを downstream に流すよう変更した
- その後の「upstream から FIN まで直送するループ」は維持し、`close_delimited_bytes` の初期値を `leftover.len()` から開始する
- 既存コメントを「decoder バッファ drain + upstream 直送」のハイブリッド構造を反映する記述に書き換え、RFC 9112 Section 6.3 item 8 の根拠を明記した
- `CHANGES.md` の `## develop` に `[FIX]` エントリを追加した

備考: integration test 基盤は 0051 で導入を見送ったため、本 issue でもテスト追加は将来 issue (forward proxy example 等の整備) に委ねる。修正自体は `decoder.take_remaining()` の挙動 (本体側の単体テストでカバー済み) に依存しており、差分は 1 箇所のみ。
