# 0042: MultipartParser の inner_delimiter 直後判定を遅延し再入時の永久 Incomplete を防ぐ

Created: 2026-05-12
Model: Opus 4.7

## 概要

`MultipartParser::next_part` の `InPart` ブランチで、パート本体を切り出した直後に `inner_delimiter` (`\r\n--<boundary>`) の **末尾 2 バイト** を見て close-delimiter (`--`) か新パート区切り (`\r\n`) かを判定するが、`after_next + 2 > buffer.len()` の場合は判定をスキップして `pos` だけ進め、`state = InPart` のまま Part を返す。次回 `next_part()` 呼び出し時に `InPart` ブランチは `\r\n\r\n` (ヘッダー区切り) を探そうとするが、追加 feed が `--\r\n` (close-delimiter 末尾) や `\r\n` (次パート区切り) だけの場合は `\r\n\r\n` が永久に出現せず `Err(MultipartError::Incomplete)` を返し続ける。

`src/multipart.rs:452-465` (実コード):

```rust
let after_next = body_end + self.inner_delimiter.len();
if self.buffer.len() >= after_next + 2 {
    if &self.buffer[after_next..after_next + 2] == b"--" {
        self.finished = true;
        self.state = ParserState::Finished;
    } else if &self.buffer[after_next..after_next + 2] == b"\r\n" {
        self.pos = after_next + 2;
    } else {
        self.pos = after_next;
    }
} else {
    self.pos = after_next;
}
```

最後の `else` 分岐 (2 バイト不足ケース) で `state` を変えずに Part を返す経路が根本原因。`InPart` 状態は本来「1 つのパートを処理中」のはずだが、Part 返却後も `InPart` のまま流用されているため、次回呼び出し時のヘッダー再探索ループに陥る。

## 根拠

### 再現 PoC

- `boundary = "boundary"`
- chunk1 = `--boundary\r\nContent-Disposition: form-data; name="a"\r\n\r\nhello\r\n--boundary` (73 byte)
- chunk2 = `--\r\n` (4 byte)

```rust
let mut parser = MultipartParser::new("boundary");
parser.feed(chunk1).unwrap();
let part = parser.next_part().unwrap().unwrap();
// part.name() == Some("a"), part.body() == Some(b"hello") を確認できる
parser.feed(chunk2).unwrap();
let result = parser.next_part();
// 期待: Ok(None) かつ parser.is_finished() == true
// 実際: Err(MultipartError::Incomplete) を永久に返す
assert!(parser.is_finished());
```

state 遷移:

| ステップ | state | pos | buffer.len | is_finished |
|---|---|---|---|---|
| chunk1 feed 後 next_part #1 (Part 返却直後) | InPart | 73 | 73 | false |
| chunk2 feed 後 next_part #2 (`\r\n\r\n` を探すが無い) | InPart | 73 | 77 | false |
| 以降 | InPart | 73 | 77 | false (永久) |

### 0035 との関係

`issues/closed/0035-fix-multipart-find-bytes-quadratic-rescan.md` の解決方法で「is_finished の遷移は本修正のスコープ外。byte-by-byte 経路では『最後のパート切り出し時に `after_next + 2` が buffer 末尾を超える』場合に終端境界 `--` の判定が後回しになり、state が InPart のまま残ることがある。これは別の改善余地として残す」と明示されている。本 issue はその「別の改善余地」の直接の後継。

### 0043 との切り分け

0043 は `Initial` ブランチ (`src/multipart.rs:355-376`) の `--<boundary>` 直後判定の RFC 違反 (transport-padding 検証欠落) を扱う。本 issue は `InPart` ブランチ (`src/multipart.rs:452-465`) の `inner_delimiter` 直後判定の遅延扱いを扱う。コード上の該当範囲は重複せず、両者は独立に修正可能。

### 影響

- 正規入力 (close-delimiter `--<boundary>--\r\n` で正しく終了する RFC 準拠の multipart) を chunk 分割で feed すると、boundary 直後で chunk が切れた瞬間に呼出側のループが永久に `Incomplete` を返し続ける
- TCP セグメント結合・TLS レコード境界・chunked transfer の chunk 境界で偶然発生する。攻撃者の意図的な細切れ feed でも発火する
- `max_buffer_size` (デフォルト 10 MB、0001 で導入) で **メモリ滞留は有限** にガード済みだが、`is_finished()` への遷移ができないため呼出側ループは抜けられない。呼出側で I/O タイムアウトを設けていない場合はハング、設けている場合もタイムアウトまで接続を占有する
- `find_bytes` の O(N·M) は 0035 で `boundary_scan_offset` 経由で線形化済みだが、本問題は計算量とは別系統の **state machine の設計欠陥**

## スコープ

本 issue は「`inner_delimiter` 直後 2 バイトが揃うまで state 遷移を遅延する」 **だけ** に絞る。以下は **含まない**:

- `feed_eof()` 相当の API 追加: 現状の `is_finished()` で完了判定可能、本 issue の修正後は `is_finished() == false && 上流 EOF` の判定で「データ不足エラー」と上位層で識別できる。EOF 専用 API が必要なら別 issue で扱う
- `Initial` ブランチの transport-padding 検証 (0043 で対応)
- preamble / epilogue の取り扱い (RFC 2046 §5.1.1 で epilogue は許容、現状の扱いは別 issue で扱う)

## 対応方針

### `src/multipart.rs`

`ParserState` に `AfterInnerDelimiter` 状態を追加し、`InPart` ブランチで Part 返却前に state を `AfterInnerDelimiter` に遷移する。

```rust
enum ParserState {
    Initial,
    InPart,
    AfterInnerDelimiter,  // 新設: inner_delimiter 直後の 2 バイト判定中
    Finished,
}
```

L452-465 の修正方針:

- `buffer.len() >= after_next + 2` のケース: 従来通り即時に `Finished` / `InPart` (次パートのヘッダー parse へ) に遷移して Part を返す
- `buffer.len() < after_next + 2` のケース: state を `AfterInnerDelimiter` に遷移し、`pos = after_next` を保存して Part を返す

`AfterInnerDelimiter` ブランチを `next_part()` の loop に追加し、2 バイト揃ったら:

- `b"--"` → `state = Finished, finished = true`、次の呼び出しで `Ok(None)` を返す
- `b"\r\n"` → `state = InPart`、`pos += 2` で次パートのヘッダー parse に遷移
- それ以外 → `Err(MultipartError::InvalidBoundary)` (0043 と整合)

`buffer.drain` (L470-478) の発動条件は維持し、Part 返却時に必要分を drain する。drain 後も `AfterInnerDelimiter` 状態の `pos` 値は drain 後の buffer 上の相対位置として整合させる。

### 既存テストの厳格化

`tests/test_multipart.rs::test_multipart_parser_byte_by_byte_feed_matches_bulk_feed` の「注: byte-by-byte 経路では…is_finished の遷移までは検証しない」コメントを **削除** し、`is_finished()` の遷移までを assertion に含めるよう厳格化する。このコメントは本 issue が修正すべき挙動をテストから意図的に除外している箇所で、修正完了の検証として削除が必須。

### テスト戦略 (AGENTS.md の分業)

- 単体テスト (`tests/test_multipart.rs`):
  - `test_multipart_parser_close_delimiter_split_after_inner_delimiter`: chunk1 = `...\r\n--<boundary>` (close-delim 末尾 2 バイト直前で切断)、chunk2 = `--\r\n` で Part 返却 + `is_finished() == true` を検証
  - `test_multipart_parser_next_part_split_after_inner_delimiter`: chunk1 同上、chunk2 = `\r\n<次パート>` で次パートが正常 parse されることを検証
  - close-delimiter 末尾 1 バイトだけ来たケース (`--` の 1 文字目) の挙動も追加
- PBT (`pbt/tests/prop_multipart.rs`): 任意の境界で chunk 分割した入力に対して、ラウンドトリップ (`feed → next_part のループ → 元のパートと一致 + is_finished == true`) を検証
- Fuzzing (`fuzz/fuzz_targets/fuzz_multipart.rs`): 現状は 1 回 feed のみ。本 issue のスコープ外 (fuzz の chunk-split 拡張は別 issue で検討)

### CHANGES.md

`## develop` に `[FIX]` として追加する:

```
- [FIX] `MultipartParser::next_part` の `InPart` ブランチで inner_delimiter 直後 2 バイトが揃わないケースに `AfterInnerDelimiter` 状態を新設し、再入時の永久 Incomplete を防ぐ
  - 旧実装は Part 返却後も `state = InPart` のまま `pos` を進めるだけで、後続 feed で close-delimiter (`--\r\n`) や次パート区切り (`\r\n`) しか来ない場合に `\r\n\r\n` を永久に探し続けるバグ経路を持っていた
  - chunk 境界で発生する DoS 経路 (TCP セグメント結合・TLS レコード境界・chunked transfer の chunk 境界、攻撃者の細切れ feed) を遮断する
  - 同時に `tests/test_multipart.rs::test_multipart_parser_byte_by_byte_feed_matches_bulk_feed` の「is_finished の遷移は未検証」注記を削除し、byte-by-byte 経路でも `is_finished()` まで検証するよう厳格化する
  - @voluntas
```

### ブランチ

`feature/fix-multipart-fragment-after-boundary-incomplete-loop` (`feature/fix-` prefix、後方互換あり)。

## 受け入れ基準

- `ParserState` に `AfterInnerDelimiter` (または同等の中間状態) が追加されている
- `tests/test_multipart.rs` に close-delimiter 末尾 chunk 切断と次パート区切り chunk 切断の単体テストが追加されている
- `tests/test_multipart.rs::test_multipart_parser_byte_by_byte_feed_matches_bulk_feed` の「注」コメントが削除され、`is_finished()` までを検証している
- `pbt/tests/prop_multipart.rs` に任意の境界で chunk 分割するラウンドトリップ PBT が追加されている
- `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` がすべて PASS
- CHANGES.md `## develop` に `[FIX]` エントリが追加されている

## 関連 issue

- 0034 (multipart Initial 状態の終端境界判定 off-by-one): 修正済み、本 issue とは別経路
- 0035 (find_bytes の二次オーダー再走査): 修正済み、本 issue は 0035 の「別の改善余地として残す」とされた箇所の対応
- 0043 (dash-boundary 直後 transport-padding 検証欠落): 姉妹 issue、`Initial` ブランチ側の修正で独立
- 0001 (multipart parser buffer limit): `max_buffer_size` 経由でメモリ滞留はガード済み

## RFC 参照

- RFC 7578 §4.1 (multipart/form-data、boundary delimiter)
- RFC 2046 §5.1.1 (multipart の delimiter / dash-boundary / close-delimiter)
  - 注: `refs/` 配下に RFC 2046 はないため、別途参照する。`close-delimiter := delimiter "--"` (RFC 2046 §5.1.1) が本 issue の「`--<boundary>` 直後 2 バイト」判定の根拠
