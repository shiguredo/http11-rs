# 変更履歴

- UPDATE
  - 後方互換がある変更
- ADD
  - 後方互換がある追加
- CHANGE
  - 後方互換のない変更
- FIX
  - バグ修正

## develop

- [ADD] RFC 9110 Section 15 の全ステータスコードに対応する `canonical_reason()` 関数を追加する
  - @voluntas
- [CHANGE] `MultipartParser::feed()` の戻り値を `Result<(), MultipartError>` に変更する
  - バッファ上限超過時に `MultipartError::BufferOverflow` を返す
  - @voluntas
- [ADD] `MultipartParser` にバッファ上限を追加する
  - `max_buffer_size` フィールドを追加し、デフォルト 10MB の上限を設ける
  - `with_max_buffer_size()` ビルダーメソッドを追加する
  - @voluntas

### misc

- [ADD] `feed_unchecked()` と `DecoderLimits::unlimited()` に未信頼入力での OOM リスクを警告するドキュメントを追加する
  - @voluntas

## 2026.1.0

**リリース日**: 2026-02-25
