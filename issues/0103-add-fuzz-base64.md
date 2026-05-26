# base64.rs の fuzz ターゲットを追加する

- Priority: High
- Created: 2026-05-26
- Model: Opus 4.7
- Branch: feature/add-fuzz-base64

## 目的

`src/base64.rs` の `decode` は空白除去 → パディング検証 → 余剰 bit 検証の多段パーサーであり、Basic / Digest 認証の credential canonicalization に使われる。任意入力に対するパニック安全性とラウンドトリップの正当性を fuzzing で検証する必要がある。

## 優先度根拠

- `decode` は認証ヘッダー経由で外部入力が直接流入する経路
- RFC 4648 Section 3.5 のストリクト検証は多段の境界条件 (パディング個数、余剰 bit、空白除去後の長さ) を持ち、エッジケースでのパニックが起こり得る
- `encode` → `decode` のラウンドトリップ不整合は credential の不正解釈につながる
- `pub(crate)` のため既存 fuzz ターゲットからは `fuzz_auth` 経由でしか間接到達できない

## 現状

- `base64.rs` にはインラインの `#[cfg(test)]` テストが存在する (encode/decode の基本ケース、エラーケース)
- PBT / fuzzing は存在しない

## 設計方針

- `fuzz/fuzz_targets/fuzz_base64.rs` を新規作成する
- 2 パターンで検証する:
  1. **パニック安全性**: 任意 `&str` を `decode` に投入し、パニックしないことを検証
  2. **ラウンドトリップ**: 任意 `&[u8]` を `encode` → `decode` し、元の入力と一致することを検証
- `pub(crate)` のため `fuzz_auth` と同様に `shiguredo_http11` の公開 API 経由 (例: `Authorization::parse` / `WwwAuthenticate` のパース) で間接的にファズするか、base64 を直接呼べる経路を用意する

## 完了条件

- `fuzz/fuzz_targets/fuzz_base64.rs` が作成されている
- `fuzz/Cargo.toml` に `[[bin]]` エントリが追加されている
- `cargo fuzz build fuzz_base64` が成功する
- パニック安全性とラウンドトリップの両方がカバーされている
