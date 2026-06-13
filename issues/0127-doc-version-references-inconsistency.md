# SKILL.md / CHANGES.md / サンプル内のバージョン表記や RFC 参照が不整合

- Priority: Low
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/doc-version-references-inconsistency
- Polished: {YYYY-MM-DD}

## 目的

SKILL.md、CHANGES.md、README.md、サンプルコード内の HTTP バージョン表記、RFC 番号、廃止 RFC への参照を整理し、最新の RFC 9110 / 9112 / 9111 に統一する。

## 優先度根拠

Low とする。動作に直接影響はないが、ドキュメントの信頼性と保守性を損ないうる。AGENTS.md では RFC 準拠を最優先とし、RFC 7230/7231 は廃止された旨が明記されている。

## 現状

- `CHANGES.md` 等に RFC 7230 / 7231 への参照が残っている可能性がある。
- サンプルコメントに古い RFC 番号が混在している可能性がある。
- `skills/shiguredo-http11/SKILL.md` にも同様の参照が含まれる可能性がある。

## 設計方針

1. RFC 7230 は RFC 9112、RFC 7231 は RFC 9110、RFC 7234 は RFC 9111 へ置き換える。
2. 文脈に応じて廃止を明示する。
3. サンプルのコメントも最新化する。

## 完了条件

- ドキュメント・コメント内の RFC 番号が最新に統一されること。
- 廃止 RFC への参照が削除または補足説明付きで置き換えられること。
- テストは不要（ドキュメント変更）。

## 解決方法

- `CHANGES.md`、`README.md`、`skills/shiguredo-http11/SKILL.md`、`examples/` 内のコメントを検索し、RFC 番号を更新する。
- `AGENTS.md` の指針に従い、コメントは日本語を基本とする。
