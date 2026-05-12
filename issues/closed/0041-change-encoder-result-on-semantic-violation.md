# 0041: Request::encode と Response::encode を Result 化して意味論違反を fail-fast にする

Created: 2026-05-12
Completed: 2026-05-12
Model: Opus 4.7

## 概要

`Request::encode` / `Response::encode` / `Request::encode_headers` / `Response::encode_headers` (`src/encoder.rs:858-862, 878-882, 1095-1099, 1112-1116`) は、内部の `Result<Vec<u8>, EncodeError>` を `.expect(...)` で握り潰しており、構築時に検出されない違反が release ビルドで panic に化ける経路を持つ。これらを `Result<Vec<u8>, EncodeError>` 直返しに統一して fail-fast にする。

並行して提供されている `Request::try_encode` / `Response::try_encode` / `Request::try_encode_headers` / `Response::try_encode_headers` は本変更で同義となるため撤去する。

## 根拠

### release panic 経路

`Request::new` / `Response::new` / `with_version` / `with_status` / `header` / `add_header` / `set_header` / `body` / `set_body` / `clear_body` / `without_body` の構築 API は構文レベル (token / VCHAR / status code / version 文字列) しか検査しない。`EncodeError` (`src/error.rs:58-116`) のうち以下 15 バリアントは構築時に検出されず encode 時のみ検出される。`encode()` を呼ぶといずれも `.expect()` で release panic に化ける:

`MissingHostHeader`, `DuplicateHostHeader`, `InvalidHostHeader`, `HostAuthorityMismatch`, `NonEmptyHostWithoutAuthority`, `UserinfoInHttpUri`, `EmptyHostInHttpUri`, `ConflictingTransferEncodingAndContentLength`, `ForbiddenTransferEncoding`, `ForbiddenContentLength`, `ForbiddenBodyFor205`, `ContentLengthMismatch`, `InvalidRequestTargetForm`, `InvalidContentLengthValue`, `DuplicateContentLength`

加えて `pub(crate) fn from_raw_parts` (decoder 経由の構築) は debug ビルドでのみ契約検査 (`debug_assert!`) を行うため、release ビルドでは構文バリアント (`InvalidMethod`, `InvalidRequestTarget`, `InvalidVersion`, `InvalidHeaderName`, `InvalidHeaderValue`, `InvalidStatusCode`, `InvalidReasonPhrase`) の検出もすべて encoder 側の `validate_request_fields` / `validate_response_fields` まで遅延される。reverse proxy などで decoder 由来の Head を再エンコードする経路では、これら構文系も release panic に化ける。

### 再現 PoC (公開ビルダー API のみ、`?` を受ける関数内で評価)

```rust
fn run() -> Result<(), EncodeError> {
    // Host 未設定で encode → MissingHostHeader → release panic
    let req = Request::new("GET", "/")?;
    let _ = req.encode();
    Ok(())
}

// 205 + body → ForbiddenBodyFor205 → release panic
let res = Response::with_status(StatusCode::RESET_CONTENT).body(b"x".to_vec());
let _ = res.encode();
```

### 過去判断との整理 (0017 / 0025 との関係)

`issues/closed/0017-change-response-fields-private-with-validation.md` と `issues/closed/0025-change-request-fields-private-with-validation.md` は「`encode()` のパニック条件を意味論的違反 (Content-Length 不一致等) に限定した doc に更新する」方針を採用していた。これは構築時バリデーションでは検出できない違反を encode に委ねる設計だった。

本 issue はその設計を覆す。`Result` 経路 (`try_encode`) は既に提供されており、利用者が `encode()` を誤って選択すると release プロセスが abort する経路は AGENTS.md「性能より堅牢性を優先する」「一切妥協をしないこと」と整合しない。Rust API Guidelines の C-FAILURE (失敗しうる関数は `Result` を返す) にも合致する。

## スコープ

本 issue は「`encode` 系メソッドの戻り型を `Result` に統一する」 **だけ** に絞る。以下は **含まない**:

- ビルダー段階 (`Request::header` / `Response::header` / `body` 等) でのクロスフィールド検査追加 (TE+CL 排他、205 ボディ禁止、Host 重複検出 等)。`Request::body` / `Response::body` を `Result` 化すると `issues/closed/0039` の `body() infallible` 方針と衝突し、また `header()` と `body()` の呼び出し順依存を生むため設計判断を別 issue に委ねる
- `from_raw_parts` 経路の構文バリデーション強化 (decoder 由来の Head に対する `release` での再検証)。別 issue で扱う
- 自由関数 `encode_request` / `encode_response` / `encode_request_headers` / `encode_response_headers` (`src/lib.rs:93-96` で公開) は既に `Result<Vec<u8>, EncodeError>` を返すため、戻り型変更なし

## 対応方針

### `src/encoder.rs`

- `Request::encode` / `Response::encode` / `Request::encode_headers` / `Response::encode_headers` の 4 メソッドを **撤去**
- `Request::try_encode` / `Response::try_encode` / `Request::try_encode_headers` / `Response::try_encode_headers` を `encode` / `encode_headers` に **リネーム** (戻り型は既存の `Result<Vec<u8>, EncodeError>` をそのまま維持)
- 撤去対象の `.expect("invalid request fields or headers")` / `.expect("invalid header combination")` および panic 条件を述べる doc コメント (現 `Request::encode` / `Response::encode` / `Request::encode_headers` / `Response::encode_headers` 上部) は消滅する
- リネーム後 `encode` / `encode_headers` の doc には「構築時バリデーションを通った値でも意味論違反で `Err` を返す」旨を明記する

### 呼び出し側の書き換え

| 場所 | 現状 | 書き換え方針 |
|---|---|---|
| `src/lib.rs` doc 例 | `let bytes = request.encode();` を `Vec<u8>` 想定 | `?` を返す `fn main() -> Result<...>` 形にして `?` 伝播 |
| `README.md` | `.encode()` 5 箇所 | 同上 |
| `skills/shiguredo-http11/SKILL.md` | `.encode()` 5 箇所 | 同上 |
| `examples/http11_client/src/transport.rs` | request encode | `?` 伝播 |
| `examples/http11_server/src/main.rs` | response encode | `?` 伝播 + 500 応答へのフォールバック |
| `examples/http11_server_io_uring/src/main.rs` | `responses.push_back((response.encode(), should_keep_alive))` | `?` 伝播のため関数シグネチャを `Result<_, EncodeError>` に変更 |
| `examples/http11_reverse_proxy/src/main.rs` | request/response 再エンコード 4 箇所 | `?` 伝播 + upstream エラー時の 502 応答 |
| `tests/test_request.rs` / `tests/test_response.rs` / `tests/test_encoder.rs` | `.encode()` | `.encode().unwrap()` (テスト内のみ許容) |
| `pbt/tests/prop_*.rs` | `.encode()` / `.try_encode()` | `.encode().expect(...)` または `prop_assert!(result.is_ok())` |
| `fuzz/fuzz_targets/fuzz_decoder_roundtrip.rs` | `try_encode()` | `encode()` にリネーム |

`fuzz/fuzz_targets/fuzz_encode_request.rs` / `fuzz_encode_response.rs` は自由関数 `encode_request` / `encode_response` を呼んでおり戻り型変更なし、修正不要。

### テスト戦略 (AGENTS.md の分業)

- PBT (`pbt/tests/prop_encoder.rs`): 構築時バリデーションを通った Request / Response に対する encode のラウンドトリップ性質を検証
- 単体テスト (`tests/test_encoder.rs`): 意味論違反を含む Request / Response が `Err(EncodeError::*)` を返すエラーパスをバリアント別に検証
- Fuzzing (`fuzz/fuzz_targets/fuzz_encode_request.rs` / `fuzz_encode_response.rs`): 既に Result 経路を呼びパニック安全性を検証済み、本 issue で追加変更なし

### CHANGES.md

`## develop` に `[CHANGE]` として追加する:

```
- [CHANGE] `Request::encode` / `Response::encode` / `Request::encode_headers` / `Response::encode_headers` の戻り型を `Result<Vec<u8>, EncodeError>` に変更する
  - 旧 `try_encode` / `try_encode_headers` を撤去し名前を `encode` / `encode_headers` に統一する
  - 構築時バリデーションを通っても意味論違反 (Host 欠落、TE+CL 同時、205 ボディ等) で encode が `Err` を返すため、呼出側は `?` 等で伝播する必要がある
  - @voluntas
```

同時に、既存の `[CHANGE] Response の全フィールドを非公開化し、構築時バリデーションを追加する` エントリ内 (issue 0017 由来) の `Response::encode() のパニック条件を意味論的違反 (Content-Length 不一致等) に限定した doc に更新する` 記述は本変更で無効化されるため削除する。

### ブランチ

`feature/change-encoder-result-on-semantic-violation` (`feature/change-` prefix、issue 番号を含まない、AGENTS.md「後方互換のない変更は prefix を `feature/change-`」に準拠)。

## 受け入れ基準

- `Request::encode` / `Response::encode` / `Request::encode_headers` / `Response::encode_headers` の戻り型がすべて `Result<Vec<u8>, EncodeError>` になっている
- `try_encode` / `try_encode_headers` が src / examples / tests / pbt / fuzz から物理的に消えている (grep ヒット 0)
- `src/encoder.rs` 内の `.expect("invalid request fields or headers")` / `.expect("invalid header combination")` が消えている (grep ヒット 0)
- panic 条件を述べる doc コメントが新 `encode` の doc に置き換わっている
- examples / tests / pbt / fuzz のすべての呼び出し側が `?` または `unwrap` 経由で Result を扱うよう書き換わっている
- `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` がすべて PASS
- CHANGES.md `## develop` に `[CHANGE]` エントリが追加され、0017 由来「Response::encode() のパニック条件 ... doc を更新する」表記が削除されている

## 関連 issue

- 0017 / 0025: Response / Request のフィールド非公開化。本 issue で「意味論違反は doc で panic を許容する」方針を覆す
- 0039: `Request` の `body()` を infallible に固定する方針。本 issue ではビルダー検査を含めないため衝突なし
- 0044 / 0045 / 0046: HRS 経路の修正。本 issue の Result 化により呼出側で smuggling 検知エラーを伝播できるようになる

## 解決方法

- `src/encoder.rs` の `Request::encode` / `Response::encode` / `Request::encode_headers` / `Response::encode_headers` を `Result<Vec<u8>, EncodeError>` 直返しに変更し、`.expect(...)` 経路と panic 条件を述べる doc を削除した
- 同一機能だった `Request::try_encode` / `Response::try_encode` / `Request::try_encode_headers` / `Response::try_encode_headers` を撤去した
- 呼び出し側 (`src/lib.rs` doc 例、`README.md`、`skills/shiguredo-http11/SKILL.md`、`examples/http11_client` / `http11_server` / `http11_server_io_uring` / `http11_reverse_proxy`、`tests/test_response.rs`、`pbt/tests/prop_request.rs` / `prop_response.rs` / `prop_encoder.rs` / `prop_decoder/*.rs`、`fuzz/fuzz_targets/fuzz_decoder_roundtrip.rs`) を `?` または `.unwrap()` 経由で Result を扱う形に書き換えた
- `CHANGES.md` の `## develop` に `[CHANGE]` エントリを追加し、issue 0017 由来「`Response::encode()` のパニック条件 ... doc を更新する」表記を削除した
