# 0035: MultipartParser の find_bytes が断片入力で O(N²) 再走査になるのを修正する

Created: 2026-05-12
Model: Opus 4.7

## 概要

`src/multipart.rs::find_bytes` は `haystack.windows(needle.len()).position(...)` で線形探索する。次の 2 つの問題がある:

1. **1 回あたりの計算量**: `windows().position()` は最悪 O(N·M) 比較を行う。`N = haystack.len()`、`M = needle.len()`
2. **断片入力での再走査**: `feed()` で 1 バイトずつ追加された場合、`next_part()` は毎回 `&self.buffer[self.pos..]` 全体を最初から再走査する。`max_buffer_size = 10MB` のとき悪意ある入力で O(N²·M) = 約 700MB 以上の比較を引き起こせる (DoS リスク)

`MultipartParser` は受信側として「断片的に到来するバイト列」を許容する Sans I/O 設計なので、断片入力での再走査コストは現実的な問題。`max_buffer_size` で絶対的な暴走は防げるが、その範囲内で攻撃者が CPU を浪費させる経路を生む。

## 根拠

- レビュー指摘: 多パート multipart で 10 MB のバッファに boundary を含まないペイロードを 1 バイトずつ feed されると、`find_bytes` が毎 feed 毎にバッファ全体を再走査する
- CLAUDE.md「サンプルは『お手本』なので性能と堅牢性を両立させること」「依存は最小限にすること」

## 対応方針

### `src/multipart.rs::find_bytes`

- 最初のバイトの出現位置を `iter().position()` で探してから needle 全体を比較する形式に書き換える
- 最悪計算量は O(N·M) のままだが、boundary が稀なバイト (`\r`) で始まる場合の定数倍を削減する

### `MultipartParser`

- 境界 (`first_delimiter` / `inner_delimiter`) 検索の再開位置を保持する `boundary_scan_offset: usize` フィールドを追加する (絶対オフセット)
- `Initial` と `InPart` の境界検索で `&self.buffer[start..]` を `start = max(self.pos, self.boundary_scan_offset)` から開始する
- 検索失敗時、`self.boundary_scan_offset = self.buffer.len().saturating_sub(needle.len() - 1)` で次回再開位置を保存する (overlap 分は再走査する)
- 検索成功時または状態遷移時 (`pos` を進めるとき) に `boundary_scan_offset` を更新する
- `feed()` 後に同じ `find_bytes` を呼ぶケースで、前回失敗位置から再開できる

### 依存

`memchr` クレートは導入しない (CLAUDE.md「依存は最小限」)。標準ライブラリのみで実装する。

### テスト

- `tests/test_multipart.rs`: 断片入力 (1 バイトずつ feed) で巨大バッファ末尾に終端境界がある場合に、`next_part()` が finished を返すまでの動作が正しいことを確認
- 計測テストは PBT のスコープ外 (時間計測は環境依存)。Property としては「断片入力でも一括入力と同じ結果」だけを検証
- `pbt/tests/prop_multipart.rs`: 断片サイズを変えても同じパース結果を得ること

### CHANGES.md

`## develop` のメインに `[FIX]` として追記する。バグ修正 (パフォーマンス起因の DoS リスク低減)。
