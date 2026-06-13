# Cache-Control の no-cache / private 修飾が情報喪失する

- Priority: Medium
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-cache-control-no-cache-private-args
- Polished: 2026-06-13

## 目的

`Cache-Control: no-cache="field-name"` および `private="field-name"` の修飾（qualified form）の引数を保持し、再エンコードできるようにする。

## 優先度根拠

Medium とする。RFC 9111 Section 5.2.2.4 / 5.2.2.7 では `no-cache` / `private` は引数なしの形式に加え、`#field-name` 引数を持つ修飾形式を定義している。現状の `CacheControl` は `no_cache` / `private` を `bool` として保持しており、修飾形式の引数が喪失してしまう。

## 現状

- `src/cache.rs` の `CacheControl` は `struct` である。`no_cache` / `private` フィールドは `bool` になっており（76 行目 / 92 行目付近）、修飾形式の引数を保持できない。
- `CacheControl::parse` は `input.split(',')` でディレクティブを分割している（123 行目付近）ため、`private="Set-Cookie, Authorization"` のような quoted-string 内のカンマを誤って区切りと認識する。
- 修飾形式の引数は `no_cache = true` / `private = true` に集約され、引数内容は破棄される（167 行目 / 170 行目 / 176 行目 / 184 行目付近）。
- `fmt::Display` は引数なしの `no-cache` / `private` のみ出力する（372 行目 / 396 行目付近）。

## 設計方針

1. `CacheControl` の `no_cache` / `private` フィールドを引数を保持できる形に変更する。引数なしの従来のフラグも維持するため、例えば `no_cache: bool` に加えて `no_cache_args: Option<String>` を持たせる。
2. 引数文字列は quoted-string 内の生の値（DQUOTE は除く）として保持し、再エンコード時には `escape_quotes` 等で quoted-string 形式に戻す。RFC 9111 は token 形式の生成を SHOULD NOT としているため、送信側は quoted-string 形式を用いる。
3. `is_no_cache()` / `is_private()` は引数なし・引数ありのいずれの場合も `true` を返す。
4. 引数付き用の builder (`with_no_cache_args`, `with_private_args`) と getter (`no_cache_args()`, `private_args()`) を追加する。引数なし用の `with_no_cache()` / `with_private()` はそのまま維持する。
5. ディレクティブ分割には `src/validate.rs` の `split_with_quotes` を利用し、quoted-string 内のカンマを無視する。quoted-string のパースには `parse_quoted_string` を利用する。受信時に token 形式（DQUOTE なし）で渡された値も生の文字列として保持し、送信側は quoted-string 形式で再エンコードする。
6. obs-text (0x80-0xFF) を含む引数は opaque data として保持する（AGENTS.md）。

## 完了条件

- `no-cache="Set-Cookie"` / `private="Set-Cookie, Authorization"` がパースされ、`is_no_cache()` / `is_private()` が `true` になること。
- `no_cache_args()` / `private_args()` で修飾形式の引数文字列を取得できること。
- 上記を再エンコードしたとき、元の quoted-string 形式（`no-cache="Set-Cookie"` / `private="Set-Cookie, Authorization"`）に戻ること。
- 引数なしの `no-cache` / `private` も引き続きパース・エンコードできること。
- `max-age=3600, private="Set-Cookie"` のように修飾形式と他のディレクティブが混在しても正しく扱えること。
- テストが追加されること（`tests/test_cache.rs`）。
- `CHANGES.md` に変更内容が記載されること。

## 解決方法

- `src/cache.rs` の `CacheControl` 構造体と `parse` / `fmt::Display` / builder / getter を修正する。
- ディレクティブ分割を `split_with_quotes` に置き換え、quoted-string 内のカンマを無視する。
- 引数付き `no-cache` / `private` の値を保持するフィールドを追加し、エンコード時に quoted-string 形式で出力する。
- `tests/test_cache.rs` に修飾形式のパース・再エンコードテストを追加する。
