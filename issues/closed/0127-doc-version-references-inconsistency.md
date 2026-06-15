# SKILL.md / CHANGES.md / サンプル内のバージョン表記や RFC 参照の最新化確認

- Priority: Low
- Created: 2026-06-13
- Completed: 2026-06-16
- Model: Kimi K2.7 Code
- Branch: feature/doc-version-references-inconsistency
- Polished: 2026-06-16

## 目的

廃止 RFC (RFC 7230 / 7231 / 7234) への参照を `README.md` / `CHANGES.md` 現行記述 / `skills/shiguredo-http11/SKILL.md` / `examples/` 内コメントから除去・最新化し、AGENTS.md の方針を補強する。

### スコープ整理 (close 推奨判定)

2026-06-16 時点で `grep -rn "RFC 7230\|RFC 7231\|RFC 7234"` を対象範囲で実施した結果、ヒットは `CHANGES.md:544` の **過去変更履歴エントリのみ** であり、これは「廃止 RFC 参照を更新した」変更記録として残すべきもの。**現行記述に廃止 RFC 参照は存在しない**。

本 issue は実質的に「現状確認の結果、修正対象なし」の状態であり、残るアクションは AGENTS.md L52-54 に既存の「RFC 7230 廃止 → 9112」「RFC 7231 廃止 → 9110」と並べて「**RFC 7234 廃止 → 9111**」の 1 行を追加するだけ。完了条件もこれに最小化する。

## 優先度根拠

Low とする。動作に直接影響はなく、現状記述に廃止 RFC 参照は存在しない。残るアクションは AGENTS.md への 1 行追記のみで、ドキュメントの信頼性と保守性を保つための最小作業。AGENTS.md L52-54 の整備として完了する。

## 現状

2026-06-13 時点で以下を確認した。

- `CHANGES.md` 内の現行の設計方針や API 仕様を説明する記述には RFC 7230 / 7231 / 7234 への参照は残っていない。
  - 2026.4.0 の misc エントリ（544 行目付近）に「廃止 RFC 参照 (`RFC 7230`) を RFC 9110 Section 5.6.2 に更新する」という過去の変更履歴があるが、これは変更記録として残す。
- `README.md` の「規格書」セクションは RFC 9110 / 9111 / 9112 / 3986 / 6265 等の最新 RFC 参照に統一されている。
- `skills/shiguredo-http11/SKILL.md` も RFC 9110 / 9111 / 9112 等の最新 RFC 参照に統一されている。
- `examples/` 内のコメントも RFC 9110 / 9112 等の最新 RFC 参照に統一されている。
- `refs/` 内の一次資料テキストや、RFC 7616 / RFC 7617 / RFC 8187 / RTSP RFC 7826 等の参照文献内の `RFC 7230` 等の引用は、元文書の参考情報として残す。

HTTP/1.0 / HTTP/1.1 / RTSP/1.0 / RTSP/2.0 等のプロトコルバージョン文字列は、リクエストライン / ステータスライン、テストデータ、サンプル出力として正しく使用されている。

## 設計方針

1. RFC 7230 は RFC 9112、RFC 7231 は RFC 9110、RFC 7234 は RFC 9111 へ置き換える。
2. 文脈に応じて廃止を明示する。
3. サンプルのコメントも最新化する。
4. `refs/` 内の一次資料や他 RFC 内の参考文献に含まれる廃止 RFC 表記は、元文書の歴史的情報として残す。

## 完了条件

- `AGENTS.md` L52-54 周辺に「RFC 7234 は廃止されて RFC 9111 になってる」の 1 行を追記する (既存の「RFC 7230 は廃止されて RFC 9112 になってる」「RFC 7231 は廃止されて RFC 9110 になってる」と並べる)。
- `grep -rn "RFC 7230\|RFC 7231\|RFC 7234"` で対象範囲 (`README.md` / `CHANGES.md` 現行記述 / `skills/` / `examples/`) を再検証し、ヒットが `CHANGES.md:544` の過去変更履歴エントリのみであることを確認する。
- `CHANGES.md` への記載は不要 (AGENTS.md 自身の編集はリポジトリ運用ドキュメントの修正であり、CHANGE/ADD/UPDATE/FIX いずれにも該当しない。`shiguredo-changelog` 規約上の対象外)。

## 解決方法

1. `AGENTS.md` L52-54 周辺の RFC 廃止リストに RFC 7234 を追加する。
2. `grep -rn "RFC 7230\|RFC 7231\|RFC 7234"` で確認する。
3. 確認結果を PR description に記載して close する。

## 補足: close 推奨

本 issue の作業は AGENTS.md への 1 行追記のみで、独立した `feature/doc-version-references-inconsistency` ブランチを切るほどの規模ではない。他の AGENTS.md 改訂と同 PR で扱うか、ユーザー判断で本 issue を `close` 経由で履歴に残すことも検討できる。

## close 理由 (2026-06-16)

2026-06-16 時点で `grep -rn "RFC 7230\|RFC 7231\|RFC 7234"` を `README.md` / `CHANGES.md` 現行記述 / `skills/` / `examples/` に対して再実行した結果、ヒットは `CHANGES.md:544` の過去変更履歴エントリのみであり、現行記述に廃止 RFC 参照は存在しないことを確認した。残作業は AGENTS.md L52-54 への RFC 7234 廃止 1 行追記のみで、独立 PR を切る規模ではないため、ユーザー判断で本 issue を close する。RFC 7234 廃止記述の AGENTS.md 追記は、他の AGENTS.md 改訂 (例: 0108 や 0116 で予定される改訂) と同 PR で実施するか、別途軽微な doc PR で扱う。
