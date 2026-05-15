# SetCookie の Max-Age 負値を 0 にクランプし、Path 属性の空値を None として扱う

- Priority: Medium
- Created: 2026-05-15
- Model: deepseek v4-pro

## 目的

1. `SetCookie::parse` の `Max-Age` 属性が負値を `i64` としてそのまま受理する。RFC 6265 Section 5.2.2 は負の Max-Age を 0 として扱うべきと規定している。
2. `Path` 属性が空値をそのまま `Some("")` として格納する。RFC 6265 Section 5.2.4 は空の attribute-value に対して default-path を使うべきと規定している。

## 優先度根拠

- RFC SHOULD/MUST 要件だが、実際のブラウザ挙動への影響は限定的
- 両方とも `SetCookie` の単一関数内の修正で完結する

## 現状

`src/cookie.rs:273-282` (Max-Age):
```rust
if let Ok(seconds) = attr_value.parse::<i64>() {
    set_cookie.max_age = Some(seconds);
}
```

`src/cookie.rs:304-306` (Path):
```rust
set_cookie.path = Some(attr_value.to_string());
```

## 設計方針

1. `Max-Age`: 負値の場合は `max_age = Some(0)` にクランプする
2. `Path`: 空値または先頭が `/` でない場合は `None` のままにする (default-path は呼び出し側の責務)

## 完了条件

- `Max-Age=-3600` が `max_age = Some(0)` になること
- `Path=` (空値) が `path = None` になること
- `cargo test` で全テストが通過すること
- `CHANGES.md` の `## develop` に `[FIX]` エントリが追加されていること
