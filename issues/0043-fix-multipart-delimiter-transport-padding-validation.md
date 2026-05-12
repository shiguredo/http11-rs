# 0043: MultipartParser の dash-boundary 直後で transport-padding CRLF 検証欠落を修正する

Created: 2026-05-12
Model: Opus 4.7

## 概要

`MultipartParser::next_part` の `Initial` ブランチ (`src/multipart.rs:355-376`) で、`--<boundary>` 直後のバイトが `\r\n` (次パート区切り) でも `--` (close-delimiter) でもない場合に、そのまま `InPart` に遷移してパースを続行している。

```rust
// src/multipart.rs:355-376
if self.buffer.len() >= after_delim + 2 {
    if &self.buffer[after_delim..after_delim + 2] == b"\r\n" {
        self.pos = after_delim + 2;
        self.boundary_scan_offset = self.pos;
        self.state = ParserState::InPart;
    } else if &self.buffer[after_delim..after_delim + 2] == b"--" {
        self.state = ParserState::Finished;
        self.finished = true;
        return Ok(None);
    } else {
        // 本 issue が修正対象とする RFC 違反経路
        self.pos = after_delim;
        if self.buffer[self.pos..].starts_with(b"\r\n") {
            self.pos += 2;
        }
        self.boundary_scan_offset = self.pos;
        self.state = ParserState::InPart;
    }
}
```

RFC 2046 §5.1.1 の ABNF は `dash-boundary transport-padding CRLF body-part` で、`transport-padding = *LWSP-char` (SP / HTAB) の後に **必須 CRLF**、または close-delimiter (`dash-boundary "--"`)。`--<boundary>` 直後に SP/HTAB 以外のバイトが直接続く入力は不正。

ただし L370-371 の「先頭の CRLF があればスキップ」分岐は、直前の `b"\r\n"` 一致ブランチ (L356-360) で既に CRLF が消費されているため到達不能 (else 分岐到達時に `starts_with(b"\r\n")` は必ず false)。

## 根拠

### 再現 PoC

```rust
let mut parser = MultipartParser::new("b");
let input = b"--bContent-Disposition: form-data; name=\"a\"\r\n\r\nhello\r\n--b--\r\n";
parser.feed(input).unwrap();
let part = parser.next_part().unwrap().unwrap();
// 期待: Err(MultipartError::InvalidPart) (または同等の RFC 違反エラー)
// 実際: part.name() == Some("a"), part.body() == Some(b"hello") として受理される
let next = parser.next_part().unwrap();
assert!(next.is_none() && parser.is_finished());
```

`--b` を index 0 で発見、`after_delim = 3`、`buffer[3..5] = "Co"` → CRLF でも `--` でもないため L366 else 分岐 → `pos = 3`、`state = InPart` → `\r\n\r\n` 発見 → ヘッダー parse 成功 → 正常 Part として返却される。

### Parser differential (filter bypass の足場)

- フロント (WAF / proxy) が「`--<boundary>\r\n` で始まらない → preamble の一部」と判定する一方、本実装は有効なパートとして読む
- multipart 仕様準拠の他実装 (Python `email.parser` / Go `mime/multipart` 等) と挙動が異なるため、複数 parser 経由でデータが流れる経路で smuggling 様の不一致を起こす
- multipart/form-data の Content-Disposition フィルタを迂回するペイロード混入の足場

### RFC との整合

RFC 2046 §5.1.1 の関連 ABNF (本 issue で必要な箇所のみ):

```
dash-boundary := "--" boundary
delimiter := CRLF dash-boundary
close-delimiter := delimiter "--"
transport-padding := *LWSP-char
```

RFC 2046 §5.1.1 には「The use of `transport-padding` is **NOT RECOMMENDED**, but the BNF allows it for the sake of robustness. Implementations **MUST** be able to parse it」とあり、送信側は生成禁止寄り、受信側はロバストネス原則で寛容受理する設計。本 issue の修正方針は「SP/HTAB を 0 個以上スキップしたうえで CRLF / `--` のいずれでもなければ不正」とし、ロバストネス原則と整合する。

注: `refs/` 配下に RFC 2046 はないため別途参照する。実装コードに RFC 2046 §5.1.1 の節番号と「将来変更される可能性がある」旨のコメントを残す (AGENTS.md「資料を由来の機能を実装する場合は、根拠資料名、節番号、将来変更される可能性があることをコードコメントで明記する」)。

## スコープ

- `Initial` ブランチ (`src/multipart.rs:355-376`) の `--<boundary>` 直後判定のみを扱う
- 含まない:
  - `InPart` ブランチの `inner_delimiter` 直後判定 (0042 で対応)
  - close-delimiter `--<boundary>--` 直後の transport-padding (現状の 0034 で「CRLF は OPTIONAL」と整理済み、本 issue では触らない)
  - preamble / epilogue の取り扱い

## 対応方針

### `src/multipart.rs::next_part` (Initial ブランチ)

L366-376 の else 分岐を以下に置き換える:

1. `after_delim` から SP/HTAB を 0 個以上スキップする (transport-padding の寛容受理)
2. スキップ後の位置から 2 バイトを判定:
   - `b"\r\n"` → `pos = padding 後 + 2`、`state = InPart` (次パートのヘッダー parse へ)
   - `b"--"` → `state = Finished, finished = true`、`Ok(None)` を返す
   - それ以外 → `Err(MultipartError::InvalidPart)` を返す (boundary 文字列自体の不正は `InvalidBoundary` で混同しないよう用途分離)
3. スキップ中または判定時にバッファ末端に到達した場合は `Err(MultipartError::Incomplete)` を返し、`pos` を進めずに次回 feed 後に再開する

L370-371 の死コード (`starts_with(b"\r\n")` スキップ) は削除する。直前の `b"\r\n"` 一致ブランチで CRLF は必ず消費済みのため到達不能。

### `MultipartError` のバリアント整理

現状の `MultipartError::InvalidBoundary` (`src/multipart.rs` 内) は `is_valid_boundary()` の boundary 文字列構文不正専用で使われている (L303, L551 周辺)。本 issue の修正で「`--<boundary>` 直後の不正バイト」を同一バリアントで返すと用途が混在するため、以下のいずれかを採用する:

- 案 A: `InvalidPart` バリアント (既存) を流用する
- 案 B: `InvalidDelimiter` バリアントを新設する

実装者の判断で選択し、PR 説明にどちらを採ったか明記する。

### ParserState への影響

`ParserState` enum 自体は変更しない (Initial ブランチ内の判定ロジック変更のみ)。バッファ末端到達時の `Incomplete` 返却で `state = Initial` のまま維持し、次回 feed 後に再判定する。0042 の `AfterInnerDelimiter` 状態追加とは独立に実装可能。

### テスト戦略 (AGENTS.md の役割分担)

- 単体テスト (`tests/test_multipart.rs`):
  - `--<boundary>X` の `X` を個別ケースで verify: `\r` 単独 / `-` 単独 / `\0` / VCHAR の代表値 (`A`, `9`) / 制御文字 (`\x01`) / 非 ASCII (`\x80`)
  - transport-padding 寛容受理: `--<boundary> \t\r\n<body>`, `--<boundary>\t\r\n<body>` が正常 parse されること
  - close-delimiter 直前の transport-padding (`--<boundary>-- \t\r\n`) は **本 issue のスコープ外、テストしない**
- PBT (`pbt/tests/prop_multipart.rs`): `--<boundary>` 直後に SP/HTAB 以外の任意バイト列が続く入力で reject される性質を検証
- Fuzzing: 既存の `fuzz/fuzz_targets/fuzz_multipart.rs` でカバー済み (panic 安全性のみ)

### CHANGES.md

`## develop` に `[FIX]` として追加する:

```
- [FIX] `MultipartParser` の `Initial` ブランチで `--<boundary>` 直後の transport-padding CRLF / close-delimiter 検証欠落を修正する
  - 旧実装は `--<boundary>X` (X が CRLF / `--` でない任意バイト) をそのまま `InPart` 状態に遷移して有効パートとして parse していた (RFC 2046 §5.1.1 違反)
  - WAF / フロントプロキシが preamble の一部と判定する一方、本実装は有効パートとして読む parser differential を生み、Content-Disposition フィルタ迂回の足場となっていた
  - SP / HTAB を 0 個以上スキップした上で CRLF / `--` のいずれでもないバイト列を `MultipartError::InvalidPart` (または新設 `InvalidDelimiter`) として reject するよう変更する
  - 死コードだった L370-371 の `starts_with(b"\r\n")` スキップを削除する
  - @voluntas
```

### ブランチ

`feature/fix-multipart-delimiter-transport-padding-validation` (`feature/fix-` prefix、後方互換あり)。

## 受け入れ基準

- `src/multipart.rs::next_part` の `Initial` ブランチ L366-376 の else 分岐が RFC 違反入力に対して `Err(MultipartError::InvalidPart)` (または `InvalidDelimiter`) を返す
- 死コードだった `starts_with(b"\r\n")` スキップが削除されている
- `tests/test_multipart.rs` に `--<boundary>X` の `X` 個別ケース (`\r` / `-` / `\0` / `A` / `\x01` / `\x80`) の reject 単体テストが追加されている
- `tests/test_multipart.rs` に SP/HTAB の transport-padding を伴う正常入力のテストが追加されている
- `pbt/tests/prop_multipart.rs` に RFC 違反入力に対する reject PBT が追加されている
- `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` がすべて PASS
- CHANGES.md `## develop` に `[FIX]` エントリが追加されている

## 関連 issue

- 0034 (multipart Initial 状態の終端境界判定 off-by-one): 修正済み、close-delimiter の CRLF OPTIONAL 化を整理済み
- 0042 (multipart fragment after boundary incomplete loop): 姉妹 issue。`InPart` ブランチ側、`AfterInnerDelimiter` 状態追加で独立に実装可能
- 0035 (find_bytes quadratic rescan): 修正済み、本 issue とは別経路

## RFC 参照

- RFC 2046 §5.1.1 (multipart の dash-boundary / delimiter / close-delimiter / transport-padding)
- RFC 7578 §4.1 (multipart/form-data、boundary delimiter)
- 注: `refs/` 配下に RFC 2046 はないため、別途参照する。実装コードに RFC 2046 §5.1.1 を明記するコメントを残す
