# 0056: CHANGES.md `## develop` セクションを規約順に並び替え種別タグ欠落を補完する

Created: 2026-05-13
Model: Opus 4.7

## 概要

AGENTS.md (`CLAUDE.md`) は以下を規約として明記している:

- エントリは種別の順番を守って記載すること (UPDATE → ADD → CHANGE → FIX の順)
- 各エントリは `- [種別] 変更内容を〜する` というフォーマットで書く

`/review-code` のレビューで CHANGES.md `## develop` セクションに以下の規約違反が検出された:

1. **本セクション (L14-242) の順序違反**: `[CHANGE]` → `[FIX]` → `[CHANGE]` → `[FIX]` → `[UPDATE]` → `[ADD]` → ... と完全に混在
2. **misc サブセクション (L247-291) の順序違反**: `[FIX]` → `[UPDATE]` → `[ADD]` → ... と混在
3. **L282 のエントリに `[種別]` タグ欠落**: `- CI を ci (全 OS) と e2e-test (ubuntu-24.04 のみ) の 2 job に分割し ...` が種別タグ無しで書かれている

`## 2026.3.0` 等の既存リリース節は変更しない (歴史的な記録のため)。

## 根拠

### AGENTS.md 引用

```
- エントリは種別の順番を守って記載すること（UPDATE → ADD → CHANGE → FIX の順）
- 各エントリは `- [種別] 変更内容を〜するという形で書く` というフォーマットにすること
```

### 0056 がリリース準備の前提

`2026.4.0` 正式リリース時に `## develop` を `## 2026.4.0` + `**リリース日**: YYYY-MM-DD` に書き換える際、規約違反のままだとリリースノートに乗ったまま外部に出る。リリース前に整形する必要がある。

## スコープ

### 対象

- `CHANGES.md` の `## develop` セクション (本セクション + `### misc` サブセクション) のみ
- `## 2026.3.0` 以下の既存リリース節は変更しない

### 並び替え方針

- `## develop` 本セクションのエントリを `UPDATE → ADD → CHANGE → FIX` 順に並び替える
- `### misc` サブセクションも同順に並び替える
- 同じ種別内では現状の順序 (時系列) を可能な限り維持する (Git 履歴との対応を保つため)
- L282 の CI 分割エントリに `[UPDATE]` 種別タグを付与する (CI 設定変更 = 機能影響なし = `[UPDATE]` が妥当)

### 対象外

- エントリの内容 (本文) の修正は本 issue では行わない
- 分類見直し (`[FIX]` を `[CHANGE]` に変える等) は別 issue で扱う
- examples 系エントリの misc 移動も別 issue で扱う

## 対応方針

### 並び替え後の構造

`## develop` 本セクション (現状 41 件):

- UPDATE (2): `Request` impl Into, `Response` impl Into
- ADD (5): `Request::set_body`, `Response::set_body`, `Response::set_omit_body`, `StatusCode`, `http11_server --port 0`
- CHANGE (15): 公開 enum non_exhaustive, auth-param cap, CONNECT 2xx TE/CL drop, `HttpHead::content_length`, `encode` Result 化, `Request::add_header`, response TE+CL error, trailer whitelist, RequestHead/ResponseHead 非公開化, RequestDecoder tunnel API, examples feature 撤廃, `Response::add_header`, `Request` 非公開化, `Response` 非公開化, `set_expect_no_body` 撤去, `is_informational` 撤去 / StatusClass
- FIX (19): TE/Trailer trim_ows, reverse_proxy close-delimited, CONNECT 405, --upstream, io_uring kTLS, io_uring pipelined, TE HTTP/1.1, MultipartParser inner_delimiter, MultipartParser Initial, `decode_headers` request_method, Base64 strict, `is_keep_alive` HTTP/1.1, DigestAuth username*, quoted-string strict, Cache-Control partial quote, MultipartParser find_bytes, MultipartParser off-by-one, `peek_body_decompressed`, Content-Length trim_ows

`### misc` サブセクション (現状 8 件):

- UPDATE (4): `is_valid_reason_phrase`, doc RFC 節番号, CI 分割 (種別タグ付与), Decompressor 実装
- ADD (2): http11_server curl integration test, http11_client testcontainers integration test
- FIX (2): 日本語化 (0055), `is_keep_alive` doc RFC 節番号

### CHANGES.md

本 issue 自体に対応する CHANGES.md エントリは追加しない。CHANGES.md の整形作業は「機能・コード変更を伴わない」「規約遵守のための整理」であり、CHANGES.md に記載する変更ではない (メタ作業のため)。

### ブランチ

`feature/fix-changes-md-reorder-and-type-tag` (`feature/fix-` prefix、機能影響なし、issue 番号を含まない)。

## 受け入れ基準

- `## develop` 本セクションのエントリが `UPDATE → ADD → CHANGE → FIX` の順に並んでいる
- `### misc` サブセクションのエントリも同順に並んでいる
- `## develop` セクション全エントリが `- [種別] ...` で始まっている (種別タグ欠落 0 件)
- 各エントリの本文と担当者 (`- @voluntas`) の対応関係が崩れていない
- `## 2026.3.0` 以下の既存リリース節は変更されていない
- `git diff --stat develop` で変更ファイルが `CHANGES.md` と `issues/closed/0056-...md` の 2 ファイルのみ
- `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace --all-targets` がすべて PASS (コード変更なしなのでベースラインから変化しない)

## RFC 参照

- 本 issue は RFC 仕様に依存しない (CHANGES.md フォーマット規約に基づく整形作業)
