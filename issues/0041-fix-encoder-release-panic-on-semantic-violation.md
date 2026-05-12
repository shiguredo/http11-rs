# 0041: Request::encode と Response::encode の意味論違反時 release panic 経路を塞ぐ

Created: 2026-05-12
Model: Opus 4.7

## 概要

`Request::encode` / `Response::encode` および `Request::encode_headers` / `Response::encode_headers` は、内部で `encode_request` / `encode_response` / `encode_request_headers` / `encode_response_headers` を呼び、その `Result<Vec<u8>, EncodeError>` に対して `.expect(...)` を発火させている。

```rust
// src/encoder.rs:860-862
pub fn encode(&self) -> Vec<u8> {
    encode_request(self).expect("invalid request fields or headers")
}
// src/encoder.rs:880-882
pub fn encode(&self) -> Vec<u8> {
    encode_response(self).expect("invalid header combination")
}
```

`Request::new` / `Response::new` / `with_version` / `with_status` / `header` / `add_header` / `set_header` / `body` のいずれも **構文レベル** (token / VCHAR / status code / version 文字列など) のみを検査しており、以下の 15 種類の `EncodeError` バリアントは構築時には検出されない。これらはすべて encode 時にのみ検出されるため、`encode()` を呼ぶと release ビルドで panic する経路となる:

- `MissingHostHeader`
- `DuplicateHostHeader`
- `InvalidHostHeader`
- `HostAuthorityMismatch`
- `NonEmptyHostWithoutAuthority`
- `UserinfoInHttpUri`
- `EmptyHostInHttpUri`
- `ConflictingTransferEncodingAndContentLength`
- `ForbiddenTransferEncoding`
- `ForbiddenContentLength`
- `ForbiddenBodyFor205`
- `ContentLengthMismatch`
- `InvalidRequestTargetForm`
- `InvalidContentLengthValue`
- `DuplicateContentLength`

## 根拠

### 再現可能な PoC (公開ビルダー API のみ、unsafe 不要)

1. `Request::new("GET", "/")?.encode()` — Host 未設定 → `MissingHostHeader` で panic
2. `Request::new("CONNECT", "/")?.header("Host", "example.com:443")?.encode()` — CONNECT + origin-form → `InvalidRequestTargetForm` で panic
3. `Request::new("POST", "/")?.header("Host", "example.com")?.header("Content-Length", "5")?.body(b"hi".to_vec()).encode()` — CL 不一致 → `ContentLengthMismatch` で panic
4. `Response::with_status(StatusCode::RESET_CONTENT).body(b"x".to_vec()).encode()` — 205 + body → `ForbiddenBodyFor205` で panic

### AGENTS.md との衝突

- 「性能より堅牢性を優先すること」
- 「Premature Optimization is the Root of All Evil」
- 「一切妥協をしないこと」

`encode()` の戻り型が `Vec<u8>` で `Result` ではないため、呼び出し側に Result 経路を強制できない。`try_encode()` が並走しているが、利用者が誤って `encode()` を選んだ瞬間に本番 panic に至る。サンプル `examples/http11_reverse_proxy` のように長時間稼働するサーバーで上位入力由来の値を Request / Response に組み立てる経路では DoS 起点となる。

### ドキュメント自認

`src/encoder.rs:854-859, 875-879, 1093-1096, 1112-1113` のコメントが「意味論的 RFC 違反がある場合パニックする」と明示しており、開発者は panic 経路の存在を認知している。「documented panic だから OK」は AGENTS.md の方針と整合しない。

## 影響範囲

- `encode()` は `Vec<u8>` を返す公開 API で、利用者が `Result` 経路を強制されない
- 構築時バリデーションを通った値が encode で panic することは「型契約の破綻」
- サーバ / プロキシで上位入力由来の値を再構築する経路で `encode()` を選ぶと、入力次第で release プロセスが panic abort

## 対応方針

優先度順:

1. `Request::encode` / `Response::encode` / `Request::encode_headers` / `Response::encode_headers` の 4 メソッドを削除し、`try_encode` / `try_encode_headers` を `encode` / `encode_headers` にリネームする (戻り型を `Result<Vec<u8>, EncodeError>` に統一)。`canary` リリース期間中なので破壊的変更を行う。
2. 並行してビルダー段階 (`Request::header` / `Response::header` / `body` / `set_body`) で「TE+CL 排他」「205 ボディ禁止」「1xx/204 への TE/CL 禁止」「Host 重複検出」のクロスフィールド検査を導入し、「構築できたものは encode 可能」という型不変式を確立する。
3. `src/encoder.rs:854-859, 875-879, 1093-1096, 1112-1113` の panic 経路を正当化するドキュメントコメントを撤去する。

### テスト

- 構築時検査の追加分に対応した単体テスト + PBT
- `examples/` 各種が `try_encode` のリネーム後 API に追従しているか
- fuzz/fuzz_targets/fuzz_encode_request.rs / fuzz_encode_response.rs で意味論違反入力に対して panic しないことを確認 (現状の fuzz は構文違反のみカバー)

### CHANGES.md

`## develop` に `[CHANGE]` として追加する。

### 破壊的変更

- 公開 API の `encode()` / `encode_headers()` が `Result` を返すよう変更されるため、呼び出し側の書き換えが必要
- `try_encode()` 名は廃止 (`encode()` に統合)
- canary リリース中なので破壊的変更は許容範囲
