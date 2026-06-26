# RFC 9651 Structured Fields の fuzz target を追加する

- Priority: Medium
- Created: 2026-06-26
- Completed:
- Model: Kimi K2.7 Code
- Branch: feature/add-structured-fields-fuzz
- Polished:

## 目的

RFC 9651 Structured Fields モジュールのパース・シリアライズに対して fuzzing を行い、メモリ安全上の問題やパニック、無限ループ、非冪等なラウンドトリップを発見する。

## 優先度根拠

Structured Fields は外部からの入力を直接パースするため、悪意ある入力による panic やメモリ枯渇、パース結果の不整合がセキュリティ問題につながる。`Accept-Query` には既存の fuzz 対象がない状態で、新規モジュールから防御を始める。

## 現状

- `fuzz/fuzz_targets/` には `fuzz_decoder_header_bomb` 等の decoder 系 fuzz target があるが、Structured Fields 用の fuzz target は存在しない
- `accept_query.rs` には fuzz 対象がない

## 設計方針

- `fuzz/fuzz_targets/fuzz_structured_fields_roundtrip.rs` を新設する
- 入力を `SfList` / `SfDictionary` / `SfItem` としてパースし、シリアライズして再度パースした結果が等しくなることを検証する
- `libfuzzer-sys` の `fuzz_target!` を使用する
- 入力長の上限を設け、巨大入力によるリソース消費を防ぐ
- パース失敗も有効な入力として扱い、失敗後に panic しないことを確認する
- 必要に応じて `fuzz_structured_fields_bare_item.rs` 等を追加する

## 完了条件

- `cargo fuzz run fuzz_structured_fields_roundtrip` が実行できること
- 一定時間 fuzzing を実行して crash / panic / timeout が発生しないこと
- 既存の fuzz target が壊れていないこと

## 解決方法

- `fuzz/Cargo.toml` に新規 fuzz target を追加する
- `fuzz/fuzz_targets/fuzz_structured_fields_roundtrip.rs` を実装する
- `libfuzzer-sys` 経由で Structured Fields パーサー・シリアライザーを呼び出す
- ラウンドトリップ検証失敗時は panic させてクラッシュとして検出する
