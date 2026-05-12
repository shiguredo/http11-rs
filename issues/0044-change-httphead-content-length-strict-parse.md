# 0044: HttpHead::content_length を decoder/body と整合した厳格パースに統一する

Created: 2026-05-12
Model: Opus 4.7

## 概要

`HttpHead::content_length` (`src/decoder/head.rs:117-120`) は `Content-Length` ヘッダーの **最初の値だけ** に `.parse::<u64>()` を直接適用しており、decoder/body 側の厳格パース (`src/decoder/body.rs:1354 parse_content_length` / `:1377 parse_content_length_value`) と挙動が乖離している。複数行・OWS・カンマリストでの解釈差が HTTP Request Smuggling (CWE-444) の温床となる。本 issue は `HttpHead::content_length` の戻り型を `Result<Option<u64>, Error>` に変更し、smuggling 検知を trait 越しに伝播できるようにする (公開 API への破壊的変更)。

```rust
// src/decoder/head.rs:117-120 (現状)
fn content_length(&self) -> Option<u64> {
    self.get_header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok())
}
```

decoder/body 側 (`parse_content_length`) は `Trailer` ヘッダーや複数行 CL を走査して同値マージ、OWS trim、ASCII digit 限定、mismatched 値 reject など RFC 9110 §8.6 / §5.6.3 / §5.6.1.2 準拠の厳格パースを実装している。

## 根拠

### 差分検証表 (実コードで再検証済み)

| 入力 (`Content-Length` の値) | 現 `head.content_length()` | `parse_content_length_value` |
|---|---|---|
| `"100"` | `Some(100)` | `Ok(100)` |
| `"+100"` | `None` (`u64::from_str` は `+` 受理しない) | `Err` (`+` は ascii_digit でない) |
| `"0100"` | `Some(100)` | `Ok(100)` |
| `" 100 "` (ASCII OWS) | **`None`** | **`Ok(100)`** (`trim_ows` で除去) |
| `"100, 100"` | `None` (`,` は数字でない) | `Ok(100)` (カンマ split + 同値マージ) |
| `"100, 101"` | `None` | `Err` (mismatched, smuggling 検知) |
| 複数行 CL `100` + `100` | `Some(100)` (`get_header` は最初の 1 件) | `Ok(Some(100))` (`parse_content_length` で同値マージ) |
| 複数行 CL `100` + `101` | **`Some(100)`** (最初の値) | **`Err`** (smuggling 検知) |

決定的に挙動が割れて smuggling 経路を生むのは:

1. `" 100 "` (ASCII OWS): trait は黙って `None`、decoder は厳格 `100`
2. **複数行 CL で mismatched values**: trait は最初の値を黙って返す、decoder は smuggling 検知で `Err` を返す

### HTTP Request Smuggling シナリオ

1. 攻撃者制御の origin が `HTTP/1.1 200 OK\r\nContent-Length: 100\r\nContent-Length: 101\r\n...` のような重複 CL を返す
2. 本実装 decoder の `parse_content_length` は `Err(Error::InvalidData(...))` でメッセージ拒否
3. しかし上位アプリ (reverse proxy 等) が `head.content_length()` 経由で長さを取得すると `Some(100)` (最初の値) を取得
4. その値で下流に CL を再生成すると、本実装の本来の判定 (拒否) と下流の境界判定 (CL=100 で読む) がずれ、smuggling が成立

### 実利用箇所

- `HttpHead::content_length` (trait method)
- `Request::content_length` (`src/request.rs:417-419` 委譲)、`Response::content_length` (`src/response.rs:514-516` 委譲) — いずれも公開 API
- `examples/http11_reverse_proxy/src/main.rs:561` で HEAD レスポンスの CL 転送に使用
- 外部利用者が `Request` / `Response` の `content_length()` を呼ぶ全経路

### `HttpHead::is_chunked` の扱い

`HttpHead::is_chunked` (`src/decoder/head.rs:132-147`) も同 trait に同居する近隣 API で、decoder 本体の `parse_transfer_encoding_for_response` が厳格パース (chunked パラメータ拒否、duplicate chunked 拒否) する一方、`is_chunked` は最後のトークンの単純判定にとどまっている。本 issue のスコープには **含めず**、別 issue で対応する。

### 関連 issue との関係

- `issues/closed/0029-fix-content-length-trim-unicode-whitespace.md`: `trim_ows` を導入して `parse_content_length_value` 側を厳格化済 (encoder / decoder)。本 issue は trait 側に残された乖離を揃える続編
- `issues/0041-change-encoder-result-on-semantic-violation.md`: encode 側の Result 化と方向性が同じ。並行して進められる
- `issues/0045-fix-connect-2xx-reject-te-cl.md`: CONNECT 2xx で TE/CL を残置する致命と組み合わさって実害が顕在化する (0045 内で「致命 4」として本 issue を参照)
- `issues/0046-fix-transfer-encoding-only-http11.md`: HTTP/0.9 等で TE 受理する致命の対極。CL 厳格化と TE 厳格化は HRS 防御の両輪

## スコープ

- `HttpHead::content_length` を `Result<Option<u64>, Error>` に変更し、`Request::content_length` / `Response::content_length` の委譲メソッドも同型に揃える
- 内部実装は既存の `parse_content_length` (`src/decoder/body.rs:1354 pub(crate)`) を再利用する
- 含まない:
  - `HttpHead::is_chunked` の厳格化 (別 issue)
  - decoder 本体の挙動 (既に厳格、変更不要)
  - encoder 側の `validate_content_length_headers` (既に厳格)

## 対応方針

### `src/decoder/head.rs`

```rust
fn content_length(&self) -> Result<Option<u64>, Error> {
    crate::decoder::body::parse_content_length(self.headers())
}
```

`parse_content_length` は既に `pub(crate)` のため新規 expose は不要。戻り値の意味論は:

- `Ok(None)`: `Content-Length` ヘッダーが存在しない
- `Ok(Some(n))`: 単一値、または複数行で同値マージされた値
- `Err(Error::InvalidData(...))`: 構文不正、mismatched values、OWS 違反、obs-text 等

### `src/request.rs` / `src/response.rs`

`Request::content_length` / `Response::content_length` の委譲メソッドの戻り型を `Result<Option<u64>, Error>` に変更する。

### 呼び出し側の書き換え

| 場所 | 書き換え方針 |
|---|---|
| `examples/http11_reverse_proxy/src/main.rs:561` | `Result` を `?` 伝播 (smuggling 検知時は 502 応答) |
| `tests/test_decoder.rs` / `tests/test_request.rs` / `tests/test_response.rs` の利用箇所 | `unwrap()` または `match` でテスト内対応 |
| `pbt/tests/prop_decoder/*.rs` / `prop_request.rs` / `prop_response.rs` | `prop_assert!(result.is_ok())` 等で性質化 |

### テスト戦略

- 単体テスト (`tests/test_decoder.rs`): 上記差分検証表の全ケースを `Request::content_length` / `Response::content_length` 経由で検証 (`" 100 "`, `"+100"`, `"0100"`, `"100, 100"`, `"100, 101"`, 複数行同値、複数行 mismatched、空 / 未設定)
- PBT (`pbt/tests/prop_decoder/head.rs` 等): decoder の `BodyKind` 判定と `head.content_length()` 戻り値の整合性。具体的には:
  - `BodyKind::ContentLength(n)` → `head.content_length() == Ok(Some(n))`
  - `BodyKind::None` (HEAD / 1xx / 204 / 304 / CL 不在) → `head.content_length() == Ok(None)`
  - `BodyKind::Chunked` → `head.content_length()` は CL 不在で `Ok(None)`、または CL があれば値を返す (chunked 優先は body 側の責務、本 trait は単に CL 値を返すだけ)
  - decoder が `Err` でメッセージ拒否するケース → `head.content_length()` も同じ条件で `Err` を返すべきだが、その入力はそもそも decoder が拒否するので `RequestHead` / `ResponseHead` が構築されない (整合性は decoder 拒否によって保たれる)

### CHANGES.md

`## develop` に `[CHANGE]` として追加する:

```
- [CHANGE] `HttpHead::content_length` / `Request::content_length` / `Response::content_length` の戻り型を `Result<Option<u64>, Error>` に変更する
  - 旧実装は `.parse::<u64>().ok()` で `Content-Length: 100, 101` のような mismatched 値を黙って `None` 化、複数行 mismatched 値の場合は最初の値を黙って返していた。decoder 本体の `parse_content_length` (smuggling 検知で `Err` 返却) と挙動が乖離しており、trait 越しに smuggling 検知がバイパスされる経路を持っていた
  - decoder の `parse_content_length` (`pub(crate)`) を trait 実装内で再利用し、OWS / カンマリスト / 複数行 / mismatched 値の解釈を decoder と統一する
  - 呼出側は `?` 等で smuggling 検知エラーを伝播する必要がある
  - @voluntas
```

### ブランチ

`feature/change-httphead-content-length-strict-parse` (`feature/change-` prefix、後方互換のない変更、issue 番号を含まない)。

## 受け入れ基準

- `HttpHead::content_length` / `Request::content_length` / `Response::content_length` の戻り型が `Result<Option<u64>, Error>` になっている
- `HttpHead::content_length` の実装本体が `parse_content_length(self.headers())` に置き換わっている
- `tests/test_decoder.rs` に差分検証表の全ケースを網羅する単体テストが追加されている
- `pbt/tests/prop_decoder/head.rs` 等に `BodyKind` と `head.content_length()` の整合性 PBT が追加されている
- 既存 examples / tests / pbt の呼び出し側がすべて `Result` 経路に対応している
- `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` がすべて PASS
- CHANGES.md `## develop` に `[CHANGE]` エントリが追加されている

## RFC 参照

- RFC 9110 §8.6 (`Content-Length = 1*DIGIT`、複数値同値マージ可、異値 reject)
- RFC 9110 §5.6.3 (OWS = `*( SP / HTAB )`、Unicode 空白は対象外)
- RFC 9110 §5.6.1.2 (empty list elements MUST be ignored)

すべて `refs/rfc9110.txt` で参照可能。
