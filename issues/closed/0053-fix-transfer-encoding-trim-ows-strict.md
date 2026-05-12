# 0053: Transfer-Encoding / Trailer の OWS 解釈で `str::trim()` を `trim_ows` に置き換え Unicode 空白による HRS 経路を塞ぐ

Created: 2026-05-13
Completed: 2026-05-13
Model: Opus 4.7

## 概要

`src/decoder/body.rs` の `parse_transfer_encoding_for_request` / `parse_transfer_encoding_for_response` / `collect_declared_trailers` が Rust 標準の `str::trim()` を使ってトークン前後の OWS を除去している。`str::trim()` は `char::is_whitespace` 基準で NBSP (U+00A0) / U+2028 / U+2000-200A 等の Unicode 空白も除去してしまうため、`is_valid_field_value` (`src/validate.rs:60`) が obs-text (0x80-0xFF) を許容してヘッダー値中に Unicode 空白の UTF-8 表現が混入できる状況下で、**TE フレーミング判定が前段プロキシと食い違う HTTP Request Smuggling (CWE-444) 経路** を残している。

CHANGES.md `## develop` (L226-230) では Content-Length 経路について「`trim_ows` を `validate` モジュールに集約し、encoder の `validate_content_length_headers` と decoder の `parse_content_length_value` で共通利用する」「両者の解釈不一致を原因とする HTTP Request Smuggling (CWE-444) 経路を塞ぐ」と宣言済み (issue 0029 で実装) だが、TE 側と Trailer 側は適用漏れになっている。CL だけ塞いで TE / Trailer を放置した非対称設計を解消する。

```rust
// src/decoder/body.rs:558-573 (Trailer)
pub(crate) fn collect_declared_trailers(headers: &[(String, String)]) -> Vec<String> {
    let mut declared = Vec::new();
    for (name, value) in headers {
        if !name.eq_ignore_ascii_case("Trailer") {
            continue;
        }
        for token in value.split(',') {
            let token = token.trim();   // <-- str::trim()
            ...
        }
    }
    declared
}
```

```rust
// src/decoder/body.rs:1254-1299 (TE request) / 1307-... (TE response)
for token in value.split(',') {
    let token = token.trim();   // <-- str::trim()
    if token.is_empty() {
        continue;
    }
    let base_coding = token.split(';').next().unwrap_or(token).trim();   // <-- str::trim()
    ...
}
```

## 根拠

### RFC 引用

```
RFC 9110 §5.6.3 (Whitespace)
OWS = *( SP / HTAB )
     ; optional whitespace
```

OWS は **SP / HTAB に限定** されており、Unicode 空白を含まない。decoder の OWS 解釈もこれに従う必要がある。

```
RFC 9112 §6.1 (Transfer-Encoding)
Transfer-Encoding = #transfer-coding
transfer-coding = "chunked" / "compress" / "deflate" / "gzip" / transfer-extension
```

```
RFC 9110 §5.6.1.2 (Lists (#rule ABNF Extension))
#element => [ element ] *( OWS "," OWS [ element ] )
```

カンマ区切りトークンの前後の OWS は **SP / HTAB のみ**。

### 攻撃シナリオ (HRS / CWE-444)

1. 攻撃者が `Transfer-Encoding: \xC2\xA0chunked` (先頭バイト NBSP の UTF-8) を含むリクエストを送信する。
2. 前段プロキシ (HAProxy / nginx 等の strict-ASCII OWS 実装) は値を `\xC2\xA0chunked` のまま判定し「`chunked` ではない」と解釈、Content-Length ベースまたは close-delimited でフレーミングする。
3. 本実装は `token.trim()` で先頭 NBSP を除去 → `chunked` と認識 → chunked フレーミングで読み始める。
4. 前段と本実装でフレーミング境界がずれ、後続リクエストを巻き込む HTTP Request Smuggling が成立する。

Trailer 経路でも同様に、`Trailer: \xC2\xA0Authorization` のような値で `Authorization` を申告済み trailer として処理する経路を作れる (本実装が trailer-section の `Authorization` を許可してしまう副次経路)。

### 関連 issue

- 0029 (closed): Content-Length の `str::trim()` → `trim_ows` 化。本 issue は同じ対策を TE / Trailer 側に適用する続編。
- 0046 (closed): TE を HTTP/1.1 完全一致以外で受理しないように厳格化。本 issue とは別経路だが TE 関連 HRS 対策の同根。
- 0032 (closed): Trailer ホワイトリスト化。Trailer 申告名の解釈に直接影響する。

## スコープ

- 該当 5 箇所の `token.trim()` / `base_coding.split(';').next().unwrap_or(token).trim()` を `trim_ows` に置き換える。
- 含まない:
  - `src/cache.rs` / `src/etag.rs` / `src/content_disposition.rs` / `src/content_language.rs` / `src/host.rs` / `src/range.rs` / `src/upgrade.rs` / `src/vary.rs` 等の他モジュールにおける `str::trim()` 利用 (HRS 致命路ではない、別 issue で検討)。
  - `encoder.rs` の `cl.trim()` (CL 値の "0" 比較、`validate_content_length_headers` でバイト検証済みのため Unicode 空白混入不能)。
  - chunked body の `chunk-size` パース (decoder 内で別途バイト単位処理済み)。

## 対応方針

### コード変更

`src/decoder/body.rs` 該当箇所:

```rust
// L565 (collect_declared_trailers)
for token in value.split(',') {
    let token = trim_ows(token);
    ...
}

// L1262, L1269 (parse_transfer_encoding_for_request)
for token in value.split(',') {
    let token = trim_ows(token);
    if token.is_empty() {
        continue;
    }
    let base_coding = trim_ows(token.split(';').next().unwrap_or(token));
    ...
}

// L1317, L1324 (parse_transfer_encoding_for_response) - 同上
```

`trim_ows` は `src/validate.rs:290` に既存 (issue 0029 で導入)、`pub(crate)` でインポート済み (`use crate::validate::trim_ows`)。

### コードコメント追加

各 `trim_ows` 呼び出しの直上に以下のコメントを追加する:

```rust
// RFC 9110 Section 5.6.3 OWS = *( SP / HTAB ) に準拠して SP / HTAB のみ除去する。
// str::trim() は Unicode 空白 (NBSP / U+2028 等) を除去してしまい、前段プロキシ
// との解釈不一致による HTTP Request Smuggling (CWE-444) の足場となる。
```

AGENTS.md「資料を由来の機能を実装する場合は、根拠資料名、節番号、将来変更される可能性があることをコードコメントで明記する」に従う。

### テスト戦略

- 単体テスト (`tests/test_decode_body.rs` または `tests/test_decoder.rs`):
  - `Transfer-Encoding: \u{00A0}chunked` (NBSP UTF-8 前置) → `Error::InvalidData` ("invalid Transfer-Encoding: not a valid token") を期待
  - `Transfer-Encoding: chunked\u{00A0}` (NBSP 後置) → 同上
  - `Transfer-Encoding:  ,chunked` (空要素 + chunked) → 引き続き受理 (リグレッション防止、空要素は無視)
  - `Transfer-Encoding: \tchunked\t` (HTAB) → 引き続き受理
  - `Trailer: \u{00A0}Authorization` → declared trailers に `\u{00A0}authorization` (UTF-8 そのまま) が入り、後続 trailer-section の `Authorization` 申告比較で一致しない (= reject 経路に乗る)。具体的な期待値は実装後に確定。
  - 上記すべてを request 版 / response 版両方で検証する。
- PBT (`pbt/tests/prop_decoder/body.rs`):
  - property: `forall (te_value contains_unicode_whitespace) → parse_transfer_encoding_for_* が Err(InvalidData) を返す`
  - property: `forall (te_value with ASCII OWS only, valid chunked) → 既存挙動を維持`

### CHANGES.md

`## develop` に `[FIX]` として追加する:

```
- [FIX] Transfer-Encoding / Trailer の OWS 解釈で `str::trim()` を `trim_ows` に統一し Unicode 空白による HTTP Request Smuggling 経路を塞ぐ
  - 旧実装は `src/decoder/body.rs::parse_transfer_encoding_for_request` / `parse_transfer_encoding_for_response` / `collect_declared_trailers` で `str::trim()` を使用しており、NBSP (U+00A0) / U+2028 等の Unicode 空白を除去していた
  - `is_valid_field_value` は obs-text (0x80-0xFF) を許容するため、`Transfer-Encoding: \u{00A0}chunked` のような値が decoder を通過し、前段プロキシ (ASCII OWS のみ trim) との解釈不一致で HTTP Request Smuggling (CWE-444) の足場となっていた
  - RFC 9110 Section 5.6.3 OWS = *( SP / HTAB ) に準拠した `trim_ows` (issue 0029 で導入) に統一する
  - 0029 (Content-Length の trim_ows 化) と同じ対策を TE / Trailer 側に適用する続編
  - @voluntas
```

### ブランチ

`feature/fix-transfer-encoding-trim-ows-strict` (`feature/fix-` prefix、後方互換あり: reject されるようになる入力は元から RFC 違反、issue 番号を含まない)。

## 受け入れ基準

- `src/decoder/body.rs` の該当 5 箇所すべてが `trim_ows` 経由になっている
- 各 `trim_ows` 呼び出しの直上に RFC 9110 Section 5.6.3 への参照コメントが付いている
- `tests/test_decode_body.rs` または `tests/test_decoder.rs` に NBSP / U+2028 / HTAB / 空要素のケースが追加されている (request / response 両方)
- `pbt/tests/prop_decoder/body.rs` に Unicode 空白を含む TE / Trailer の reject を検証する PBT が追加されている
- `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace --exclude http11_client --exclude http11_server` がすべて PASS
- CHANGES.md `## develop` に `[FIX]` エントリが追加されている

## RFC 参照

- RFC 9110 §5.6.3 (OWS = *( SP / HTAB )、`refs/rfc9110.txt`)
- RFC 9110 §5.6.1.2 (#rule ABNF Extension の OWS 規定)
- RFC 9112 §6.1 (Transfer-Encoding 定義、`refs/rfc9112.txt`)
- RFC 9110 §5.5 (field-value と obs-text 許容範囲)

## 解決方法

- `src/decoder/body.rs::collect_declared_trailers` (L565) の `token.trim()` を `trim_ows(token)` に置き換えた
- `src/decoder/body.rs::parse_transfer_encoding_for_request` (L1262, L1269) の `token.trim()` / `base_coding ... .trim()` を `trim_ows` 経由に統一した
- `src/decoder/body.rs::parse_transfer_encoding_for_response` (L1317, L1324) も同様に `trim_ows` 経由に統一した
- 各 `trim_ows` 呼び出しの直上に RFC 9110 Section 5.6.3 への参照コメントを追加した
- `tests/test_decode_body.rs` に NBSP / U+2028 / HTAB / 空要素のケースを request / response 両方で追加した (10 ケース、Trailer 申告との照合検証も含む)
- `pbt/tests/prop_decoder/body.rs` に `prop_request_te_unicode_whitespace_rejected` / `prop_response_te_unicode_whitespace_rejected` / `prop_request_te_ascii_ows_accepted` を追加した
- `CHANGES.md` の `## develop` に `[FIX]` エントリを追加した
