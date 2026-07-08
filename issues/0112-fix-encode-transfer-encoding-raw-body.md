# encode_request / encode_response が Transfer-Encoding 付きでも生 body を連結して不正 framing を生成する

- Priority: High
- Created: 2026-07-08
- Completed: {YYYY-MM-DD}
- Model: Grok 4.5
- Branch: feature/fix-encode-transfer-encoding-raw-body
- Polished: {YYYY-MM-DD}

## 目的

一括エンコード API (`encode_request` / `encode_response` / `Request::encode` / `Response::encode`) が `Transfer-Encoding` ヘッダーと `body` を同時に持つメッセージを **成功として** 返し、chunked 形式ではない生バイトをヘッダ終端直後に連結してしまう不具合を修正する。

## 優先度根拠

High とする。

- 生成バイト列は RFC 9112 の chunked framing にならず、受信側は chunk-size 行として先頭バイトを解釈する
- 成功 `Ok(Vec<u8>)` を返すため、呼び出し側が不正 framing に気づきにくい
- TE と CL の同時送信は既に reject している一方、TE + body の意味論は未防御で非対称

## 現状

### encode_request (`src/encoder.rs`)

- TE と CL の同時存在は `ConflictingTransferEncodingAndContentLength` で拒否する
- body があり CL/TE が無いときだけ `Content-Length` を自動付与する
- その後、TE の有無に関わらず:

```rust
if let Some(body) = request.body_bytes() {
    buf.extend_from_slice(body);
}
```

### encode_response

同様に `body_will_be_encoded` が true なら body を連結する。TE があっても chunk 化しない。

### 既存テスト

`tests/test_encoder/main.rs` の
`test_encode_response_no_content_length_with_transfer_encoding` は
TE + `body(b"hello")` で **Content-Length が付かないことだけ** を検証し、
body が chunk framing になっているか / エラーになるかは見ていない。

### 正規の streaming 経路

`encode_request_headers` / `encode_response_headers` + `encode_chunk` / `RequestEncoder` / `ResponseEncoder` が chunked 送出の本経路。一括 `encode_*` が TE+body を通すと本経路と矛盾する。

## 設計方針

一括 API では次のいずれかに統一する (推奨は A)。

- **A (推奨)**: TE ヘッダーがあり、かつ `body.is_some()` のとき `EncodeError` を返す
  - メッセージは英語 (例: body must not be set when Transfer-Encoding is present; use encode_chunk)
- **B**: TE が chunked のとき内部で `encode_chunks` 相当に変換して送る
  - 一括 API の責務が肥大化し、trailer や複数 chunk の表現が曖昧になるため非推奨

TE 値そのものの厳密検証 (chunked のみ / 重複 chunked 禁止等) は本 issue の必須範囲に含めない。必要なら別 issue とする。
本 issue の最小ゴールは **「TE + body で不正 framing を成功返却しない」** こと。

## 完了条件

- `Transfer-Encoding: chunked` + `body = Some(...)` の request/response を一括 encode するとエラーになる (方針 A の場合)
- TE のみ (body なし) の encode は従来どおり成功する
- CL のみ + body 長一致は従来どおり成功する
- TE + CL 同時は従来どおり競合エラー
- 上記を単体テストで固定する (既存の「TE で CL が付かない」テストを方針に合わせて更新)
- `cargo test --all` が通る
- `CHANGES.md` の `## develop` に `[FIX]` (必要ならエラー variant 追加なら破壊的変更の有無を明記) を追記する

## 解決方法

1. `encode_request` / `encode_response` に TE 存在かつ body ありの検査を追加する
2. 必要なら `EncodeError` に専用 variant を追加する (既存 variant で足りるなら流用可)
3. `tests/test_encoder/` に成功しないこと・エラー種別を固定するテストを追加する
4. 既存の TE+body テストを更新する
5. `CHANGES.md` を更新する

## 影響範囲

- `src/encoder.rs`
- `src/error.rs` (variant 追加時)
- `tests/test_encoder/**` / 関連 PBT
- 利用側で「TE を付けた Request/Response に body を載せて `encode()` していた」コードはコンパイルは通るが実行時 Err になる (正しい失敗)
