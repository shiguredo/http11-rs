# 0052: reverse_proxy サンプルの close-delimited 経路で decoder バッファのボディ先頭バイトを取りこぼす問題を修正する

Created: 2026-05-12
Model: Opus 4.7

## 概要

`examples/http11_reverse_proxy/src/main.rs:594-619` の close-delimited 分岐は、`decoder.peek_body()` / `consume_body()` / `take_remaining()` を一切呼ばずに `upstream.read(&mut buf)` から直接転送を開始する。

```rust
// examples/http11_reverse_proxy/src/main.rs:598-619
if is_close_delimited {
    debug!("Streaming close-delimited body until connection closes");
    let mut buf = [0u8; READ_CHUNK];
    let mut close_delimited_bytes = 0usize;
    loop {
        let n = upstream.read(&mut buf).await?;     // ← decoder.buf に残った先頭バイトを見ずに直接 read
        if n == 0 {
            debug!(total_bytes = close_delimited_bytes, "Close-delimited body complete");
            break;
        }
        downstream.write_all(&buf[..n]).await?;
        close_delimited_bytes += n;
        debug!(bytes = n, "Close-delimited body chunk");
    }
    downstream.flush().await?;
    return Ok(can_reuse);
}
```

`decode_headers` は `self.buf.drain(..pos + 2)` でヘッダー終端 `\r\n\r\n` までしか drain しない (`src/decoder/response.rs:436, 497, 578`)。したがって `decode_headers` が `Some((head, BodyKind::CloseDelimited))` を返した時点で、decoder 内部バッファにはボディ先頭バイトが残ったままになる。close-delimited 分岐は decoder 内部に触らず生 `upstream.read` に切り替えるため、**残ったボディ先頭バイトは永久に下流に届かない**。

## 根拠

### 発生条件

- TCP は record 境界を保証しないため、サーバが短いレスポンス (ヘッダー + 小〜中サイズのボディ) を 1 度の `write` で送ると、ほぼ確実に 1 つの TCP セグメント、1 度の `read` で到着する
- TLS レコード境界でも同様。1 TLS レコードに header と body 先頭が同居するケースは普通
- 本実装の read バッファサイズ: `const READ_CHUNK: usize = 8192;` (L493)。`decoder.available_buf().min(READ_CHUNK)` で `mut_buf` を確保するため、最低でも 8192 バイト分は 1 read で取り込める

### 欠落バイトの行方

- `decode_headers` 完了時点で `self.buf` にはボディ先頭バイトが残っている
- peek_body 経路 (`src/decoder/body.rs:138-143`): `BodyCloseDelimited` のときに `Some(buf)` を返すため、残バイトを取り出せる設計
- close-delimited 直接 read 経路: decoder には触れず upstream から新規バイトのみ読む → decoder 内に残った先頭バイトは下流に届かない

### 影響の重大性

- close-delimited (HTTP/1.0 互換) レスポンスは Content-Length も Transfer-Encoding もないため、欠落しても下流クライアントは **「正常終了」と解釈** してしまう
- データ破損が検知不能
- 短いレスポンス (典型的な API レスポンス) ではほぼ毎回発生する

### AGENTS.md との衝突

- 「サンプルは **お手本** なので性能と堅牢性を両立させること」
- 堅牢性が満たされない決定的なデータ破損

## 影響範囲

- close-delimited レスポンスでボディ先頭が欠落
- API レスポンス・HTML レスポンス等で破損が下流に到達
- 上流が `Connection: close` を返す任意の HTTP/1.0 互換シナリオで発生

## 対応方針

### close-delimited 分岐の前に decoder バッファを drain

```rust
if is_close_delimited {
    // decoder 内部に残ったボディ先頭バイトを先に下流へ流す
    loop {
        match decoder.peek_body() {
            Some(data) if !data.is_empty() => {
                let n = data.len();
                downstream.write_all(data).await?;
                decoder.consume_body(n)?;
            }
            _ => break,
        }
    }

    // 続いて upstream から直接転送
    let mut buf = [0u8; READ_CHUNK];
    let mut close_delimited_bytes = 0usize;
    loop {
        let n = upstream.read(&mut buf).await?;
        if n == 0 { break; }
        downstream.write_all(&buf[..n]).await?;
        close_delimited_bytes += n;
    }
    downstream.flush().await?;
    return Ok(can_reuse);
}
```

### あるいは `take_remaining` 相当 API を活用

- `decoder` 側に「残バッファを 1 度に取り出す」API が既にある (CONNECT トンネル用 `take_remaining`)
- close-delimited 専用の取り出し API を追加することも検討

### テスト

- `examples/http11_reverse_proxy` の integration test (現状不在) で、close-delimited レスポンスの全バイト到達を確認する
- 特に短い (1KB 以下) レスポンスでヘッダーとボディが 1 TCP read に乗るケースを意図的に作る

### CHANGES.md

`## develop` の `### misc` に `[FIX]` として追加する。

### 関連 issue

- 0050 (upstream URL scheme/port) と 0051 (CONNECT 未対応) と並ぶ reverse_proxy 機能不全 3 連の最終
