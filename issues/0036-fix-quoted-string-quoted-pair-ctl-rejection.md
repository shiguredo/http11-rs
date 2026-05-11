# 0036: quoted-string / quoted-pair の CTL 拒否と部分引用符の対称検査

Created: 2026-05-12
Model: Opus 4.7

## 概要

RFC 9110 Section 5.6.4 の quoted-string / quoted-pair の文字集合検証が、複数のヘッダーパーサーで不十分または存在しない。

- `src/auth.rs::parse_auth_params` (line 752-767): quoted-pair で escape 中の文字を範囲チェックなしで受理。CR / LF / NUL 等を含む不正値を quoted-string として通過させる
- `src/content_disposition.rs::parse_quoted_string` (line 393-411): 同じく escape の中身を制限せず CR / LF を素通り
- `src/cache.rs` (line 129): `value.trim().trim_matches('"')` で前後どちらか片方だけに DQUOTE がある partial 引用符 (`max-age="3600`) を許容

これらは Authorization / Content-Disposition / Cache-Control が `is_valid_field_value` (obs-text 許容) を経由してきた値に対して甘く、CR/LF 注入による response splitting / log injection / credential 構造破壊などの経路を生む。

## 根拠

### RFC 9110 Section 5.6.4 ABNF

```
quoted-string = DQUOTE *( qdtext / quoted-pair ) DQUOTE
qdtext        = HTAB / SP / %x21 / %x23-5B / %x5D-7E / obs-text
quoted-pair   = "\" ( HTAB / SP / VCHAR / obs-text )
obs-text      = %x80-FF
VCHAR         = %x21-7E
```

- `qdtext` は HTAB (0x09) / SP (0x20) / 0x21 / 0x23-0x5B / 0x5D-0x7E / 0x80-0xFF (DQUOTE 0x22 と backslash 0x5C を除く可視 + 空白 + obs-text)
- `quoted-pair` の右辺は HTAB / SP / VCHAR / obs-text。**NUL や CR / LF / 他の CTL は明示的に不許可**

### 攻撃シナリオ

1. 攻撃者が `Authorization: Custom realm="evil\r\nSet-Cookie: ..."` のようなヘッダーを送る
2. `is_valid_field_value` は CR / LF (0x0D / 0x0A) は不許可だが、obs-text 経由で UTF-8 多バイトの先頭バイトとして CR / LF と同じ振る舞いを取らせるバイト列を含む値が到達する可能性は薄い (decoder 側でも CR/LF を弾く)
3. ただし quoted-pair `\\<CR>` のような構造で escape 経路をすり抜けるとして上位アプリケーションが値を別ヘッダーに転記する経路で response splitting が成立する可能性が残る
4. RFC が明示的に「CTL は不許可」と書いている以上、受理側でも明示的に拒否すべき

### Cache-Control の partial quote

- `max-age="3600` (閉じ DQUOTE なし) の場合、現状の `trim_matches('"')` は先頭の DQUOTE だけ除去して `3600` として parse する
- 攻撃シナリオではないが、仕様準拠の `quoted-string` ABNF (両端の DQUOTE) と異なるため、parse の挙動が implementation specific になる

## 対応方針

### `src/validate.rs`

- `pub(crate) fn is_qdtext_byte(b: u8) -> bool` を追加 (HTAB / SP / 0x21 / 0x23-0x5B / 0x5D-0x7E / 0x80-0xFF)
- `pub(crate) fn is_quoted_pair_byte(b: u8) -> bool` を追加 (HTAB / SP / VCHAR / obs-text = HTAB / 0x20-0x7E / 0x80-0xFF)

### `src/auth.rs::parse_auth_params`

quoted-string 解析を以下に変更:

- quoted 内のバイトが `is_qdtext_byte` を満たすか、`\\` で escape されている場合は次バイトが `is_quoted_pair_byte` を満たすことを検証
- 不正な場合は `AuthError::InvalidParameter` を返す

### `src/content_disposition.rs::parse_quoted_string`

同様に qdtext / quoted-pair の文字集合を検証して reject する。`b as char` (Latin-1 直列化) ではなくバイト単位で扱う。

### `src/cache.rs::parse`

`value.trim().trim_matches('"')` を以下に変更:

- 値が両端 DQUOTE で囲まれている場合のみ内側を取り出す
- 片側 DQUOTE のみは `CacheError::InvalidFormat` で reject
- 引用符内の値も `is_qdtext_byte` / `is_quoted_pair_byte` で検証

### `src/content_disposition.rs::escape_quoted_string`

Display 経路で送信するため、CR / LF など `is_quoted_pair_byte` を満たさない文字が含まれていた場合は escape ではなく panic (debug_assert) するか、`Display` の戻り値を `Result` に変更するか検討。本 issue では「parse 側で reject されるので escape 側に到達しない不変条件」を保ち、`debug_assert!` を入れる。

### テスト

- `tests/test_auth.rs`: CR / LF を含む quoted-pair (`Authorization: Custom realm="evil\\\r\nx"`) が `AuthError::InvalidParameter` で reject されることを確認
- `tests/test_content_disposition.rs`: 同等のテストを追加
- `tests/test_cache.rs`: partial quote (`max-age="3600`) が拒否されることを確認

### CHANGES.md

`## develop` のメインに `[FIX]` として追記する。受理範囲が縮小するため、影響範囲を明記する。

### 破壊的変更

- CR / LF / NUL 等の CTL を含む quoted-string を以前受理していた経路は reject される
- partial quote (`max-age="3600`) は reject される
- canary リリース中なので破壊的変更は許容範囲
