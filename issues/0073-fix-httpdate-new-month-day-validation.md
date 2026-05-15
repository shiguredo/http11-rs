# HttpDate::new に月別日数検証を追加し、無効な日付の構築を防ぐ

- Priority: High
- Created: 2026-05-15
- Model: deepseek v4-pro
## 目的

`HttpDate::new` が `day` の 1..=31 範囲検証のみを行い、月別の日数上限 (2 月は 28/29 日、4/6/9/11 月は 30 日) を検証していない。RFC 9110 Section 5.6.7 が参照する IMF-fixdate (RFC 5322 Section 3.3) の day は実在する日付でなければならない。`Sun, 31 Jun 1994 08:49:37 GMT` (6 月 31 日は存在しない) のような無効な日付が成功裡に構築される。

## 優先度根拠

- 全パース経路 (`parse_imf_fixdate` / `parse_rfc850_inner` / `parse_asctime`) が最終的に `HttpDate::new` を呼ぶ
- `SetCookie::parse` が無効な日付を受け入れ、`Expires` 属性として保持する。ブラウザ等のクライアントが受信した場合、未定義動作に至る

## 現状

`src/date.rs:243-280`:
```rust
pub fn new(year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> Result<Self, DateError> {
    if month < 1 || month > 12 {
        return Err(DateError::InvalidMonth);
    }
    if day < 1 || day > 31 {
        return Err(DateError::InvalidDay);
    }
    // 月別日数検証なし
```

## 設計方針

1. `new()` 内で `month` に応じた日数上限を検証する
   - 1/3/5/7/8/10/12 月は 31 日、4/6/9/11 月は 30 日
   - 2 月はうるう年の判定 (`year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)`) を行い、29 日まで許可
2. 違反時は `DateError::InvalidDay` を返す

## 完了条件

- `HttpDate::new(1994, 6, 31, 8, 49, 37)` が `Err(DateError::InvalidDay)` を返すこと
- うるう年 (`2000`) の 2 月 29 日が有効、平年 (`2001`) の 2 月 29 日が拒否されること
- 単体テスト (`tests/test_date.rs`) に月別日数の境界値テスト (2/29 うるう年/平年、4/31 拒否、6/31 拒否等) が追加されていること
- `cargo test` で全テストが通過すること
- `CHANGES.md` の `## develop` に `[FIX]` エントリが追加されていること
