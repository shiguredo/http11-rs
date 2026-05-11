# 0034: MultipartParser Initial 状態の終端境界判定 off-by-one を修正する

Created: 2026-05-12
Completed: 2026-05-12
Model: Opus 4.7

## 概要

`src/multipart.rs::next_part` の `ParserState::Initial` 分岐において、最初の境界 `--<boundary>` を見つけた後、続く 2 バイトを検査して通常パート (CRLF) / 終端境界 (`--`) / それ以外 (寛容パス) を分岐する処理がある。

該当箇所 (`src/multipart.rs:332`):

```rust
if self.buffer.len() > after_delim + 2 {
    if &self.buffer[after_delim..after_delim + 2] == b"\r\n" {
        ...
    } else if &self.buffer[after_delim..after_delim + 2] == b"--" {
        // 終了境界
        ...
        return Ok(None);
    } else {
        ...
    }
} else {
    return Err(MultipartError::Incomplete);
}
```

条件 `self.buffer.len() > after_delim + 2` は厳密超過 (`>`) になっており、`after_delim + 2 == buffer.len()` ちょうどの場合 (= `self.buffer[after_delim..after_delim + 2]` がスライス末尾で valid に取れる場合) に偽となり `Incomplete` を返す。これは `b"------WebKitFormBoundary--"` のような終端境界がバッファのまさに末尾に来た時点で feed が止まった場合、永遠に `Incomplete` を返し続ける挙動を生む。

後続の `ParserState::InPart` 分岐 (`src/multipart.rs:415`) では同等の判定が `>=` で正しく書かれている。Initial 分岐だけ off-by-one が残っている。

## 根拠

### RFC

- RFC 7578 / RFC 2046 Section 5.1.1: multipart 終端境界は `--<boundary>--` (CRLF は terminator として OPTIONAL)。
- 本クレートの multipart は受信側として「断片的に到来するバイト列」を許容する Sans I/O 設計。境界末端ぴったりで feed が止まるケースは正常入力に含まれる。

### 影響

- 攻撃シナリオではなく実装バグ。クライアントが TCP / TLS の都合でバッファ末端ぴったりで切れた multipart を送ると、サーバが永遠に `Incomplete` を返し続けて呼出側の loop がブロックされる
- 呼出側の実装によっては DoS につながる (タイムアウトなしで再 feed を待ち続ける場合)

### スライス境界

- `self.buffer[after_delim..after_delim + 2]` が安全に取れる条件は `after_delim + 2 <= buffer.len()`
- これを満たすときに判定を行うべきで、`>=` が正解

## 対応方針

### `src/multipart.rs::next_part`

`ParserState::Initial` 分岐の `self.buffer.len() > after_delim + 2` を `self.buffer.len() >= after_delim + 2` に変更する。

### テスト

- `tests/test_multipart.rs` (または `pbt/tests/prop_multipart.rs`): 終端境界 `--<boundary>--` がバッファの最末尾に位置するシナリオで `next_part` を呼び、`Ok(None)` (finished) を返すことを確認する
  - 例: `--bnd\r\n...body...\r\n--bnd--` の最後 `--` までだけ feed して、`next_part` が `Ok(None)` を返すこと
  - 旧実装ではこのケースで `Incomplete` を返すため、テストで挙動を担保する

### CHANGES.md

`## develop` のメインに `[FIX]` として追記する。バグ修正。

## 解決方法

- `src/multipart.rs::next_part` の `ParserState::Initial` 分岐で `self.buffer.len() > after_delim + 2` を `>=` に変更した。等値ケース (`after_delim + 2 == buffer.len()`) も拾うようになり、終端境界 `--<boundary>--` が feed の末尾ピッタリで止まる断片入力でも正しく `Ok(None)` (finished) を返す
- コメントで Sans I/O での断片入力対応の意図と、参照スライス `self.buffer[after_delim..after_delim + 2]` の安全条件 (`after_delim + 2 <= buffer.len()`) を明記
- `tests/test_multipart.rs` に 2 件追加:
  - `test_multipart_parser_end_boundary_at_buffer_tail_without_crlf`: `--boundary--` のみ (CRLF terminator なし) を feed して `Ok(None)` (finished) を返すこと
  - `test_multipart_parser_part_then_end_boundary_at_tail`: 通常パート + 終端境界 (CRLF terminator なし) で正しく part と finished を取れること
- `CHANGES.md` の `## develop` に `[FIX]` エントリを追加した
