# AGENTS

- Premature Optimization is the Root of All Evil
- 一切忖度しないこと
- 常に日本語を利用すること
- 全角と半角の間には半角スペースを入れること
- 絵文字を使わないこと
- RFC 準拠を最優先すること

## レビューについて

- レビューはかなり厳しくすること
- レビューの表現は、シンプルにすること
- レビューの表現は、日本語で行うこと
- レビューの表現は、指摘内容を明確にすること
- レビューの表現は、指摘内容を具体的にすること
- レビューの表現は、指摘内容を優先順位をつけること
- レビューの表現は、指摘内容を優先順位をつけて、重要なものから順に記載すること
- ドキュメントは別に書いているので、ドキュメトに付いては考慮しないこと
- 変更点とリリースノートの整合性を確認すること

## コミットについて

- 勝手にコミットしないこと
- コミットメッセージは確認すること
- コミットメッセージは日本語で書くこと
- コミットメッセージは命令形で書くこと
- コミットメッセージは〜するという形で書くこと

## サンプルについて

- サンプルは **お手本** なので性能と堅牢性を両立させること
- サンプルは RFC に準拠していること

## RFC について

- refs/ 以下を利用すること
- RFC 7230 は廃止されて RFC 9110 になってる
- RFC 7231 は廃止されて RFC 9112 になってる

## テストについて

- pbt 以下に unittest を書かないこと
- unittest は pbt で実現できないものだけを書くこと
- 単体テストのファイル名は `tests/test_<module>.rs` とし、`src/<module>.rs` に対応させること
- PBT のファイル名は `pbt/tests/prop_<module>.rs` とし、`src/<module>.rs` に対応させること
- 特定のモジュールに対応しないテストには `test_` や `prop_` プレフィックスを付けないこと
- `#[ignore]` を使わないこと
- テストファイルが長くなった場合はファイル内で `mod` を使って分割すること
  - テストが長くなるのはモジュール自体が大きすぎるサインなので `src/<module>.rs` 側の分割を検討すること
- `src/<module>/` のようにディレクトリモジュールの場合は `pbt/tests/prop_<module>/main.rs` にサブモジュール対応で分割すること

### テストの役割分担

- PBT: 型情報（Strategy）に基づいて入力を生成し、プロパティを検証する（ラウンドトリップ等）
- Fuzzing: 任意入力に対するクラッシュ耐性（パニック安全性）
- 単体テスト: 意図的なエラーパス、境界値など PBT で実現できないケース
- PBT でカバーできるものを単体テストで書かない

### カバレッジ駆動のテスト作成手順

1. 対象モジュールの PBT + 単体テストのカバレッジを llvm-cov で取得する
2. 未カバー行を分類する:
   - 正常系ロジック未カバー → PBT の strategy を修正または PBT を追加する
   - エラーパス未カバー → 単体テストまたは fuzzing で対応する
   - 到達不可能なコード → デッドコードとして削除する
3. PBT に「任意入力でパニックしないことだけを検証するテスト」を書かない（fuzzing の役割）

### カバレッジ取得コマンド例

対象モジュールに関連するテストだけを実行し、カバレッジをマージして確認する:

```bash
# 前回の計測結果をクリアする
cargo llvm-cov clean --workspace
# src/<module>.rs 内の #[cfg(test)] mod tests を実行する
cargo llvm-cov --no-report -p shiguredo_http11 --lib -- <module>
# tests/test_<module>.rs の単体テストを実行する
cargo llvm-cov --no-report -p shiguredo_http11 --test test_<module>
# pbt/tests/prop_<module>.rs の PBT を実行する
cargo llvm-cov --no-report -p pbt --test prop_<module>
# 上記すべての計測結果をマージしてレポートを出力する
cargo llvm-cov report
```

## pre-commit

- make fmt / make clippy / make check / make test を実行すること

## Rust

- 性能より堅牢性を優先すること
- PBT(Property-Based Testing) や Fuzzing で必ずテストを行うこと
