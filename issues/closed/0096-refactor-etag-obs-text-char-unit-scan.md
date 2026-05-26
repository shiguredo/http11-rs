# etag.rs の obs-text 走査をバイト単位から char 単位に統一する

- Priority: Low
- Created: 2026-05-25
- Completed: 2026-05-25
- Model: Opus 4.7
- Branch: feature/refactor-etag-obs-text-char-scan

## 目的

`src/etag.rs` の `EntityTag::parse` / `strong` / `weak` が `tag.bytes()` でバイト単位走査を行い、`is_etagc(b: u8)` で `b >= 0x80` を obs-text として許容している。他のモジュール (auth.rs, accept.rs, content_disposition.rs 等) は issue 0059 以降で char 単位走査 (`chars()`) に統一されているが、etag.rs だけ旧方式のバイト走査が残っている。

CLAUDE.md の obs-text 規約「受信時は `char_indices()` ベースで走査し UTF-8 不変条件を保つこと」に走査方式を揃える。

### 機能差について

入力は `&str` (有効な UTF-8) であるため、バイト単位検証と char 単位検証の結果は全ての有効入力に対して同一である:
- ASCII 文字 (U+0000..=U+007F): バイト値 == Unicode scalar 値であり判定結果は同一
- 非 ASCII 文字 (U+0080..=U+10FFFF): UTF-8 の全バイトが >= 0x80 であり、バイト単位でも char 単位でも obs-text として受理される
- CR/LF/NUL: バイト単位でも既に拒否されている

従って本変更は機能面で no-op であり、プロジェクト内の走査方式統一 (リファクタリング) として位置づける。

## 優先度根拠

プロジェクト内の走査方式一貫性の確保。機能的なバグはなく、セキュリティリスクもない。issue 0059 での統一作業の残件。

## 現状

変更対象箇所:

| 箇所 | 行 | 内容 |
|------|-----|------|
| `is_etagc` 定義 | 190-192 | `fn is_etagc(b: u8) -> bool` — `u8` ベースの文字種判定 |
| `EntityTag::parse` | 104 | `for b in tag.bytes()` + `is_etagc(b)` |
| `EntityTag::strong` | 124 | `for b in tag.bytes()` + `is_etagc(b)` |
| `EntityTag::weak` | 137 | `for b in tag.bytes()` + `is_etagc(b)` |

### 変更不要箇所

- `EntityTag::parse` 行 100-101: `rest[1..].find('"')` / `&rest[1..1 + end_quote]` — DQUOTE (`"`, 0x22) は ASCII であり UTF-8 継続バイト (0x80-0xBF) と競合しないため、バイトインデックスのまま正しく動作する
- `split_etag_list_raw` 行 198-216: `input.as_bytes()` でカンマ (`,`) と DQUOTE (`"`) を走査している。いずれも ASCII であり UTF-8 継続バイトと競合しないため変更不要
- `Display` impl 行 178-186: `write!(f, "\"{}\"", self.tag)` は `tag: String` をそのまま出力するだけであり走査方式と無関係

## 設計方針

### `is_etagc` を char 単位に変更

```rust
fn is_etagc_char(c: char) -> bool {
    c == '\x21' || ('\x23'..='\x7E').contains(&c) || c > '\x7F'
}
```

- `c > '\x7F'` で Unicode scalar U+0080..=U+10FFFF を obs-text として受理する (surrogate は `char` 型で表現不可能なため自動的に除外される)
- CR (`\r`) / LF (`\n`) / NUL (`\0`) は上記条件のいずれにも該当しないため拒否される

### `parse` / `strong` / `weak` の走査を `chars()` に変更

```rust
for c in tag.chars() {
    if !is_etagc_char(c) {
        return Err(ETagError::InvalidCharacter);
    }
}
```

`char_indices()` はインデックス情報が不要なため使用しない。CLAUDE.md の `char_indices()` 指定は部分文字列の切り出しが必要な経路に対するものであり、純粋な文字検証には `chars()` で十分。

## テスト戦略

### PBT

既存の `pbt/tests/prop_etag.rs` でラウンドトリップが検証されている。PBT の Strategy が obs-text を含む ETag 値を生成するよう確認し、不足があれば Strategy を拡張する。

### 単体テスト

`tests/test_etag.rs` に以下を追加:
- obs-text (U+0080 以上) を含む ETag のパース → 成功 (例: `"v\u{00A9}"` → `EntityTag { weak: false, tag: "v\u{00A9}" }`)
- obs-text を含む ETag の `Display` → 再パースのラウンドトリップ成功
- マルチバイト文字 (U+3042 "あ" 等) を含む ETag のパース → 成功
- CR/LF/NUL を含む ETag タグ → `InvalidCharacter` エラー

### Fuzzing

既存 fuzz target がパニック安全性を担保。新規追加不要。

## 完了条件

- `src/etag.rs` の `EntityTag::parse` / `strong` / `weak` が `chars()` で走査していること
- `is_etagc` が `fn is_etagc_char(c: char) -> bool` に変更されていること
- `tag.bytes()` による etagc 文字検証が残存していないこと (`split_etag_list_raw` の ASCII デリミタ走査は除外)
- 既存テスト (PBT / 単体テスト / fuzz) が全て通ること
- obs-text (U+0080 以上) を含む ETag のラウンドトリップテストが追加されていること

## 解決方法

- `src/etag.rs` の `is_etagc(b: u8) -> bool` を `is_etagc_char(c: char) -> bool` に変更した
- `EntityTag::parse` / `strong` / `weak` の走査を `tag.bytes()` から `tag.chars()` に変更した
- `tests/test_etag.rs` に obs-text ラウンドトリップ、マルチバイト文字、CR/LF/NUL 拒否の単体テスト 4 件を追加した
