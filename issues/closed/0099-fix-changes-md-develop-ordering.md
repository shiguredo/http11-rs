# CHANGES.md develop セクションの種別順序と misc 配置を修正する

- Priority: Low
- Created: 2026-05-25
- Completed: 2026-05-25
- Model: Opus 4.7
- Branch: feature/fix-changes-md-ordering

## 目的

`CHANGES.md` の `## develop` セクションが CLAUDE.md の変更履歴規約に複数箇所で違反している。リリース前に修正し、規約に準拠した状態にする。

## 優先度根拠

ドキュメント規約の整合性。機能やセキュリティへの直接的影響はないが、リリース時の変更履歴の正確性と可読性に影響する。

## 現状

### 問題 1: `### misc` セクションへの公開 API エントリの誤配置

`## develop` の `### misc` セクション (行 14-47) 内に、公開 API の破壊的変更や追加が配置されている:

移動対象 (misc から最上位へ):
- 行 18-21: `[ADD] HeaderName / Method / Scheme 型を導入...` — 公開 API の追加
- 行 22-24: `[CHANGE] HttpHead::headers() の戻り型を...変更する` — 破壊的変更
- 行 32-36: `[CHANGE] Request/Response/RequestHead/ResponseHead のヘッダー名...変更する` — 破壊的変更
- 行 37-40: `[ADD] HeaderNameError / MethodError に input フィールドを追加...` — 公開 API の追加
- 行 41-44: `[ADD] TryFrom<&'static str> / TryFrom<&'static [u8]> を...実装する` — 公開 API の追加

misc に残す (テスト追加・サンプル変更):
- 行 16-17: `[UPDATE] examples に graceful shutdown を実装する` — サンプル変更
- 行 25: `[ADD] compile_fail doctest を追加する` — テスト追加
- 行 26-31: `[ADD] PBT 整合性検証を追加する` — テスト追加
- 行 45-47: `[ADD] TryFrom / new() の受理集合一致を PBT で検証する` — テスト追加

CLAUDE.md 規約: 「機能に直接影響しない変更 (ドキュメント追加、リファクタリング等) は `### misc` サブセクションに記載すること」

### 問題 2: 種別順序違反

`### misc` セクション内の記載順が `[UPDATE]` → `[ADD]` → `[CHANGE]` → `[ADD]` → `[CHANGE]` → `[ADD]` となっている。

CLAUDE.md 規約: 「エントリは種別の順番を守って記載すること (CHANGE → ADD → UPDATE → FIX の順)」

## 設計方針

1. `### misc` 内の公開 API エントリ (`[CHANGE]` / `[ADD]` のうち公開 API に影響するもの) を `## develop` の最上位 (misc の上) に移動する
2. 最上位セクションとして `[CHANGE]` → `[ADD]` の順で並べる
3. `### misc` 内に残るリファクタリング・テスト追加エントリも `CHANGE → ADD → UPDATE → FIX` 順に並べ替える

## 完了条件

- `## develop` セクションが `[CHANGE]` → `[ADD]` → `[UPDATE]` → `[FIX]` → `### misc` の順で構成されていること
- `### misc` 内のエントリも種別順に並んでいること
- `### misc` 内に公開 API の `[CHANGE]` / `[ADD]` が含まれていないこと

## 解決方法

- `### misc` 内の公開 API エントリ (`[CHANGE] HttpHead::headers()`, `[CHANGE] Request/Response ヘッダー名変更`, `[ADD] HeaderName/Method/Scheme 導入`, `[ADD] HeaderNameError/MethodError`, `[ADD] TryFrom`) を `## develop` の最上位に移動した
- 最上位を CHANGE → ADD → FIX の順に並べ替えた
- `### misc` 内を ADD → UPDATE の順に並べ替えた
- `### misc` にはテスト追加 / サンプル変更 / リファクタリングのみ残した
