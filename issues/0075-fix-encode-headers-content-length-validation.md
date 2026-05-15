# encode_request_headers / encode_response_headers に Content-Length の ABNF 検証と body 長整合性検証を追加する

- Priority: High
- Created: 2026-05-15
- Model: deepseek v4-pro
## 目的

`encode_request_headers()` と `encode_response_headers()` は `encode_request()` / `encode_response()` と異なり、Content-Length の値が `1*DIGIT` (RFC 9110 Section 8.6) であることの ABNF 検証と、body 長との整合性検証を行っていない。特に `encode_response_headers()` ではこれらの検証が `debug_assert!` ブロック内にあり release ビルドで完全にバイパスされる。

## 優先度根拠

- ストリーミング送信 (`encode_headers` → chunked body) で不正な Content-Length ヘッダーが出力され得る
- `encode_request_headers` は TE の有無を強制しておらず、TE なしでの使用時に矛盾したメッセージが wire に出力される可能性がある
- `encode_response_headers` の防御が release ビルドで消滅する不整合

## 現状

**`encode_request_headers`** (`src/encoder.rs:951-995`):

TE+CL 競合チェック (line 965-967) のみで、CL 値の `1*DIGIT` 検証や body 長との一致検証がない。

**`encode_response_headers`** (`src/encoder.rs:1052-1069`):

```rust
debug_assert!({
    if !response.has_header("Transfer-Encoding") {
        if let Some(content_length) = response
            .content_length()
            .transpose()
            .expect("Content-Length must be valid")
        {
            // body 長検証、validate_content_length_headers 呼び出し等
        }
    }
    true
});
```

`debug_assert!` が release ビルドで除去されるため、不正な Content-Length 値が検出されずに出力される。

## 設計方針

1. `encode_request_headers` に `!request.has_header("Transfer-Encoding")` の条件で `validate_content_length_headers` と body 長比較を追加する
2. `encode_response_headers` の `debug_assert!` ブロックを削除し、`encode_response` と同様の常時検証に変更する
3. 両関数の 205 Reset Content 制約チェックも重複を整理し共通化する

## 完了条件

- `encode_request_headers()` が `Content-Length: abc` や `Content-Length: 999` + `body: b"hello"` でエラーを返すこと
- `encode_response_headers()` の Content-Length 検証が release ビルドでも有効であること
- `tests/test_encoder.rs` に `encode_request_headers` / `encode_response_headers` の Content-Length 検証テスト（`"abc"` 拒否、長不一致拒否）が追加されていること
- `cargo test` と `cargo test --release` で全テストが通過すること
- `CHANGES.md` の `## develop` に `[FIX]` エントリが追加されていること
