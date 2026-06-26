# RFC 9651 Structured Fields のテストと PBT を追加する

- Priority: Medium
- Created: 2026-06-26
- Completed:
- Model: Kimi K2.7 Code
- Branch: feature/add-structured-fields-tests
- Polished:

## 目的

RFC 9651 Structured Fields モジュールの正確性を単体テストとプロパティベーステストで担保し、パース・シリアライズのラウンドトリップと境界値を網羅する。

## 優先度根拠

Structured Fields は strict processing を旨とする仕様であり、実装の寛容さが他実装との相互運用を損なう。テスト網羅性が品質を左右する。

## 現状

- `tests/test_accept_query.rs` には `Accept-Query` 専用のテストがあるが、汎用 Structured Fields モジュールのテストが存在しない

## 設計方針

- `tests/test_structured_fields.rs` (または `tests/test_structured_fields/` ディレクトリ) を新設する
- 以下の観点でテストを追加する
  - 各 bare item 型のパース成功例・失敗例
  - List / Dictionary / Inner List / Parameters のパース成功例・失敗例
  - 全型のシリアライズ例
  - パース -> シリアライズ -> パース のラウンドトリップ
  - 境界値 (Integer 最大/最小、Decimal 精度、空 List/Dictionary 等)
- PBT は `pbt/` クレートに追加し、valid/invalid 入力を戦略として生成する
- RFC 9651 Appendix C の ABNF や Appendix D の RFC 8941 からの変更点も考慮する

## 完了条件

- `tests/test_structured_fields.rs` に RFC 9651 全型をカバーする単体テストが追加されていること
- `pbt/` に Structured Fields のラウンドトリップ PBT が追加されていること
- `cargo test` / `cargo test -p pbt` で新規テストが通ること
- カバレッジ対象外の型がないこと

## 解決方法

- テストファイルを作成し、RFC 9651 Section 4 の各項に対応するケースを追加する
- PBT 戦略では、ABNF に基づいて valid な入力を生成し、パース・シリアライズの双方向性を検証する
- 無効入力 (非 ASCII、構文エラー、範囲外数値) も拒否されることを検証する
