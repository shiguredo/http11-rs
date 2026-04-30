# 0002: feed_unchecked / DecoderLimits::unlimited のドキュメントと命名の改善

## 概要

`feed_unchecked()` と `DecoderLimits::unlimited()` はリソース制限を完全に無効化する公開 API だが、Rust の慣例との乖離と警告不足が誤用リスクを高めている。

## 問題点

### 命名の乖離

Rust コミュニティでは `_unchecked` サフィックスはメモリ安全性（境界チェックのスキップ等）を省略する意味で使われる慣例がある。`feed_unchecked()` はメモリ安全ではあるがリソース制限を外しており、慣例と意味がズレている。

### 警告の不足

- `src/decoder/request.rs:162` — `feed_unchecked()`
- `src/decoder/response.rs:206` — `feed_unchecked()`
- `src/limits.rs:34` — `DecoderLimits::unlimited()`

いずれも未信頼入力で使うと OOM に直結するが、ドキュメントにその警告が明示されていない。

## 確認箇所

PBT 内で `feed_unchecked` が制限迂回の手段として明示的に使用されている。

- `pbt/tests/prop_decoder/request.rs:309`
- `pbt/tests/prop_decoder/response.rs:457`

テスト用途として正当だが、同時に「制限を外す手段として公開されている」ことを示している。

## 対応方針

ドキュメントに「**未信頼入力に使用しないこと**」の警告を追記する。命名変更は破壊的変更になるため、まず警告の明示化を優先する。

## 解決方法

以下の 3 箇所に `# 警告` セクションを追記した。

- `src/decoder/request.rs` — `feed_unchecked()` に「未信頼入力・OOM リスク・テスト用途限定」の警告を追加
- `src/decoder/response.rs` — 同上
- `src/limits.rs` — `DecoderLimits::unlimited()` に「すべて `usize::MAX`・未信頼入力禁止」の警告を追加
