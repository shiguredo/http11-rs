# 0042: MultipartParser の dash-boundary 直後で 2 バイト不足時に永久 Incomplete を返すバグを修正する

Created: 2026-05-12
Model: Opus 4.7

## 概要

`MultipartParser::next_part` の `InPart` ブランチで、パート本体を切り出した直後に `inner_delimiter` の末尾 2 バイト (close-delimiter `--` か新パート区切り `\r\n` か) を判定できないまま Part を返却する経路がある。

```rust
// src/multipart.rs:452-467
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
    self.pos = after_next;          // <-- ここで state を InPart のまま放置
}
self.boundary_scan_offset = self.pos;
```

`else` 分岐で `state = InPart` のまま `pos = after_next` に進めて Part を返すと、次回 `next_part()` 呼び出し時に `InPart` ハンドラ (L393) は再度 `\r\n\r\n` (ヘッダー区切り) を探そうとする。後続 feed が `--\r\n` (close-delimiter 残り) や `\r\n...` (次パート先頭) しか持たない場合、`\r\n\r\n` は永久に見つからず `Err(MultipartError::Incomplete)` を返し続ける。`is_finished` も `false` のままで、アプリ側に EOF を伝える API がない。

## 根拠

### 再現フロー (実機 PoC 確認済み)

- boundary = `boundary`
- chunk1 = `--boundary\r\nContent-Disposition: form-data; name="a"\r\n\r\nhello\r\n--boundary` (73 byte)
- chunk2 = `--\r\n`

| ステップ | state | pos | buffer.len | finished |
|---|---|---|---|---|
| chunk1 feed 後 next_part #1 (Initial→InPart→Part 返却) | **InPart** (残骸) | 73 | 73 | false |
| chunk2 feed 後 next_part #2 (InPart で `\r\n\r\n` を探すが無い) | InPart | 73 | 77 | false |
| next_part #3 以降 | InPart | 73 | 77 | false (永久) |

実機実行結果: `got Part name="a" body="hello"` → `Err(Incomplete)` → `Err(Incomplete)` → 永久。

### DoS 経路

- TCP MTU / Nagle / TLS レコード境界で偶然踏むレベル。悪意なしでも発生
- 攻撃者は 1 バイトずつ feed させて conn 占有 + メモリ滞留の二重 DoS が可能
- AGENTS.md「性能より堅牢性を優先」「Premature Optimization is the Root of All Evil」と整合しない

### RFC

- RFC 2046 §5.1.1 の `dash-boundary transport-padding CRLF body-part` を境界判定するが、`dash-boundary` 直後の 2 バイトを見ないと close-delimiter (`--`) と次パート区切り (`\r\n`) を区別できない
- Sans I/O 設計上、フラグメント feed で進めない場合は state を保持して `Incomplete` を返すのが正

## 影響範囲

- form-data 受信時の DoS と truncation
- アプリ側に EOF 通知 API がなく、永久ハング検出も困難
- HRS の直接経路ではないが、accept loop でタスクが詰む

## 対応方針

### `src/multipart.rs::MultipartParser`

- L463-465 の else 分岐で `pos` / `boundary_scan_offset` を **更新せず、Part 返却もせず**、`Err(MultipartError::Incomplete)` を返す方針に変える
- もしくは `InPart` を `ReadingHeaders` と `AfterBoundary` (close-delim 判定中) の 2 状態に分割し、`AfterBoundary` で 2 バイトが揃うまで `Incomplete` を返す
- アプリが「EOF 受信時に未完了なら error」を判定できるよう `feed_eof()` 相当の API を追加する

### テスト

- `pbt/tests/prop_multipart.rs` に chunk-split パターン (1 バイトずつ feed / 任意の境界で feed) の PBT を追加
- `tests/test_multipart.rs` に「`--boundary` の末尾で chunk が切れる」境界回帰テストを追加

### CHANGES.md

`## develop` に `[FIX]` として追加する。
