# 0062 fix Connection / Transfer-Encoding の OWS 除去で str::trim() を trim_ows に統一する

Created: 2026-05-14
Model: deepseek-v4-pro

## 概要

`src/decoder/head.rs` の `is_keep_alive` / `is_chunked` と `src/encoder.rs` の 205 Content-Length 検証が `str::trim()` を使用している。`str::trim()` は `char::is_whitespace` に基づき NBSP (U+00A0) や U+2000-200A 等の Unicode 空白も除去するが、RFC 9110 Section 5.6.3 の OWS は `*( SP / HTAB )` のみと定義されている。

`is_valid_field_value` は obs-text (0x80-0xFF) を許容するため、`Connection: \u{00A0}keep-alive` や `Transfer-Encoding: \u{00A0}chunked` のような値が decoder を通過しうる。この値を `str::trim()` で評価する実装と `trim_ows` を使う前段代理で解釈不一致が生じ、HTTP Request Smuggling (CWE-444) の足場となる。

issue 0029 (Content-Length の trim_ows 化) および 0053 (Transfer-Encoding / Trailer の trim_ows 化) と同じ対策を未対応箇所に適用する。

また、encoder.rs の 205 Content-Length 検証 (`cl.trim() != "0"`) は `encode_response_headers` の release build では `validate_content_length_headers` が `debug_assert!` 内でのみ実行されるため Unicode 空白の混入経路が残る。防御層としての一貫性のため併せて `trim_ows` 化する。

## 再現手順

1. `Connection: \u{00A0}keep-alive` を含むリクエストを decoder が受理 (is_valid_field_value 通過)
2. `HttpHead::is_keep_alive` が `str::trim()` で NBSP を除去し `keep-alive` と解釈 → `true` を返す
3. 前段の reverse proxy が `trim_ows` で SP/HTAB のみ除去し `\u{00A0}keep-alive` は `keep-alive` と一致しないと判定 → 接続を close
4. 解釈不一致により HTTP Request Smuggling が成立する

## 対象ファイル

- `src/decoder/head.rs:91` (`is_keep_alive`: `token.trim()`)
- `src/decoder/head.rs:143` (`is_chunked`: `token.trim()`)
- `src/encoder.rs:772` (`encode_response`: `cl.trim()`)
- `src/encoder.rs:1041` (`encode_response_headers`: `cl.trim()`)
- `fuzz/fuzz_targets/fuzz_request_response_helpers.rs:101, 116` (参照実装の `token.trim()`)

修正対象外: cookie / auth / cache / content-type / range 等の他モジュールの `str::trim()` は Smuggling 致命路に該当しないため本 issue では扱わない。

## 推奨対応

各箇所の `trim()` を `trim_ows()` に置換する。

`src/decoder/head.rs` の import に `trim_ows` を追加:

```rust
use crate::validate::{
    is_valid_field_value, is_valid_header_name, is_valid_method, is_valid_protocol_version,
    is_valid_reason_phrase, is_valid_request_target, is_valid_status_code, trim_ows,
};
```

`src/encoder.rs` は既に `trim_ows` import 済み (line 10)。

`fuzz/fuzz_targets/fuzz_request_response_helpers.rs` の `expected_keep_alive` / `expected_chunked` も同様に `trim_ows` 化する。これらは fuzz が実装の正しさを検証するための参照実装であり、実装だけ `trim_ows` 化すると fuzz で false positive crash が発生する。

## テスト戦略

### 単体テスト

`tests/test_request.rs` または `tests/test_head.rs`:
- `is_keep_alive` に `Connection: \u{00A0}keep-alive` (NBSP 前置) を渡し `false` を確認
- `is_chunked` に `Transfer-Encoding: \u{00A0}chunked` を渡し `false` を確認
- `is_keep_alive` に `Connection: \tkeep-alive` (HTAB)、`Connection:  keep-alive` (SP) を渡し `true` を確認 (リグレッション防止)
- `is_chunked` に `Transfer-Encoding: \tchunked`、`Transfer-Encoding:  chunked` を渡し `true` を確認

`tests/test_encoder.rs`:
- 205 レスポンス + `Content-Length: \u{00A0}0` が `encode_response` / `encode_response_headers` の両経路で `ForbiddenContentLength` になることを確認
- 205 レスポンス + `Content-Length: \t0` / `Content-Length:  0` が正当に受理されることを確認

### PBT

既存 PBT が ASCII 値のみを生成しているため strategy 変更は不要。修正後は検証が厳格化される方向であり既存取材で破綻しない。

### Fuzzing

- 参照実装 `fuzz_request_response_helpers.rs` の `expected_keep_alive` / `expected_chunked` を `trim_ows` 化する
- `cargo +nightly fuzz run fuzz_keep_alive -- -max_total_time=60` で新規 crash が出ないことを確認する

## CHANGES.md

`## develop` に以下を追記する:

```
- [FIX] `Connection` / `Transfer-Encoding` のトークン OWS 除去で `str::trim()` を `trim_ows` に統一し Unicode 空白による HTTP Request Smuggling 経路を塞ぐ
  - 旧実装は `src/decoder/head.rs::is_keep_alive` / `is_chunked` で `str::trim()` を使用しており、NBSP (U+00A0) 等の Unicode 空白を除去していた
  - `is_valid_field_value` は obs-text (0x80-0xFF) を許容するため、前段プロキシ (ASCII OWS のみ trim) との解釈不一致で HTTP Request Smuggling (CWE-444) の足場となっていた
  - 併せて encoder の 205 Content-Length 検証も `trim_ows` に置換し防御層の一貫性を確保する
  - @voluntas
```

## ブランチ名

`feature/fix-connection-te-trim-ows-strict`

## 受け入れ基準

- [ ] 対象 4 箇所 + fuzz 参照実装 2 箇所が `trim_ows` に置換されている
- [ ] `make fmt && make clippy && make check && make test` が pass
- [ ] `Connection: \u{00A0}keep-alive` で `is_keep_alive` が `false` を返す
- [ ] `Transfer-Encoding: \u{00A0}chunked` で `is_chunked` が `false` を返す
- [ ] SP / HTAB は引き続き OWS として正しく除去される
- [ ] `cargo +nightly fuzz run fuzz_keep_alive -- -max_total_time=60` で新規 crash が出ない
- [ ] `CHANGES.md` に `[FIX]` エントリが追記されている
