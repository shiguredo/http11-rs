# URI の空ポート (`host:`) で host() が末尾コロンを含む誤値を返す

- Priority: High
- Created: 2026-07-08
- Completed: {YYYY-MM-DD}
- Model: Grok 4.5
- Branch: feature/fix-uri-empty-port-host-boundary
- Polished: {YYYY-MM-DD}

## 目的

`Uri::parse` が authority の空ポート (`example.com:`) を RFC 3986 どおり解釈せず、`host()` に末尾 `:` を含めて返す不具合を修正する。併せて、その誤動作を固定している単体テストを仕様準拠に直す。

## 優先度根拠

High とする。

- `host()` は Host 比較・プロキシ転送・正規化の入力になる。`example.com:` は正当な reg-name ではない
- `Host::parse` は空ポートを `InvalidPort` として拒否しており、同一入力に対するモジュール間解釈が食い違う
- 単体テストが誤動作を「実装の動作」として固定しており、回帰として残り続けている

## 現状

### parse_authority (`src/uri.rs`)

```rust
if let Some(colon_pos) = host_part.rfind(':') {
    let host_str = &host_part[..colon_pos];
    let port_str = &host_part[colon_pos + 1..];
    validate_host(host_str)?;
    if !port_str.is_empty() {
        // host_end = colon_pos 側
        return Ok((host_end, Some(port)));
    }
}
Ok((authority.len(), None)) // 空 port でも authority 全体が host 扱い
```

`http://example.com:/path` で:

- 期待: `host() == Some("example.com")`, `port() == None`
- 実際: `host() == Some("example.com:")`, `port() == None`

RFC 3986 Section 3.2.3: `port = *DIGIT` (空を許容)。host は colon より前。

### 固定テスト (`tests/test_uri.rs`)

```rust
fn test_uri_empty_port() {
    let uri = Uri::parse("http://example.com:/path").unwrap();
    // 空ポートの場合、host には : が含まれる (実装の動作)
    assert_eq!(uri.host(), Some("example.com:"));
    ...
}
```

## 設計方針

1. 空 `port_str` のときも `host_end` を colon 位置 (userinfo 補正後) にし `port = None` とする
2. IPv6 (`[::1]:`) も同様に空 port を正しく切る
3. `normalize` で空 port の `:` を省略するかは RFC 3986 Section 6.2.3 の scheme-based 正規化と合わせて実装時に決める (最低限 host() の正しさを優先)
4. `Host::parse` の空 port 拒否方針とは別 API なので、URI 側は RFC 3986 の空 port 許容を維持する (Host ヘッダの厳格さとは役割が違う)

## 完了条件

- `Uri::parse("http://example.com:/path")` で `host() == Some("example.com")`, `port() == None`
- `http://[::1]:/path` でも host に余計な `:` が付かない
- `test_uri_empty_port` が仕様準拠の期待値に更新される
- 既存の port あり / port なし / IPv6 テストが通る
- 関連 PBT があれば空 port を strategy に含めるか回帰ケースを追加する
- `cargo test --all` が通る
- `CHANGES.md` の `## develop` に `[FIX]` を追記する

## 解決方法

1. `parse_authority` の空 port 分岐で `host_end` を colon 前に設定する
2. `tests/test_uri.rs` の期待値とコメントを修正する
3. 必要なら PBT / fuzz で空 port を追加する
4. `CHANGES.md` を更新する

## 影響範囲

- `src/uri.rs`
- `tests/test_uri.rs` / `pbt/tests/prop_uri.rs`
- `host()` の戻り値に依存し `example.com:` を特殊扱いしていたコードがあれば要確認 (通常は存在しない想定)
