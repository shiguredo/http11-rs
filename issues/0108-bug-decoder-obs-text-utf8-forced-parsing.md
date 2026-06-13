# デコーダーがメッセージ行を UTF-8 として強制解釈し obs-text を喪失する

- Priority: High
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-decoder-obs-text-utf8-forced-parsing
- Polished: {YYYY-MM-DD}

## 目的

RFC 9110 / RFC 9112 において obs-text（`0x80-0xFF`）は quoted-string / reason-phrase / field-value 等で opaque data として保持すべき対象である。しかし現状のデコーダーは request-line / status-line / ヘッダー行 / トレーラー行を `String::from_utf8` で UTF-8 として強制解釈しており、非 UTF-8 な obs-text を含む有効な HTTP/1.1 メッセージを拒否している。この問題を修正し、RFC 準拠を回復する。

## 優先度根拠

High とする。RFC 9112 Section 2.2 では「HTTP メッセージは US-ASCII のスーパーセットであるエンコーディングでオクテット列として解析しなければならない（MUST）」と規定しており、現状はこの MUST 要件に違反している。また `AGENTS.md` でも「obs-text（0x80-FF）は opaque data として保持すること」と明記されているが、実装がこれを満たしていない。

## 現状

以下の箇所で `String::from_utf8(self.buf[..pos].to_vec())` 等を用いて行全体を UTF-8 として解釈している。

- `src/decoder/request.rs:345-347`（request-line）
- `src/decoder/response.rs:442-444`（status-line）
- `src/decoder/request.rs:541-543`（ヘッダー行）
- `src/decoder/response.rs:582-585`（ヘッダー行）
- `src/decoder/body.rs:508-509`（chunked のトレーラー行）

これにより、`0x80-0xFF` の単一バイトや非 UTF-8 な obs-text シーケンスを含むメッセージが `invalid UTF-8` として拒否される。例えば reason-phrase 内や field-value 内に Latin-1 的な obs-text が含まれる有効なメッセージも受理できない。

さらに `src/validate.rs:63-65` の `is_valid_field_value` は `&str` 前提のため、非 UTF-8 な obs-text はそもそも到達不能になっている。

## 設計方針

1. request-line / status-line / header-line / trailer-line のパーサーをバイト列ベースに改修する。
2. CR / LF / NUL 以外の obs-text オクテットは失わず、opaque data として保持する。
3. `String` に変換する際は `CLAUDE.md` / `AGENTS.md` にあるように `char_indices()` ベースで走査し、Unicode scalar `U+0080..=U+10FFFF`（surrogate 除く）まで opaque char として保持する。1 バイトずつ `as char` で `String` に push する経路は Latin-1 mojibake の原因となるため禁止する。
4. `validate.rs` の field-value 検証もバイト列または `char_indices()` ベースで obs-text を受理できるようにする。
5. 送信側 builder / encoder については別途検討するが、本 issue では受信側のデコーダー改修を目的とする。

## 完了条件

- obs-text（`0x80-0xFF`）を含む有効な HTTP/1.1 メッセージがデコーダーで受理されるようになること。
- 既存の UTF-8  only メッセージのデコード挙動が変わらないこと。
- NUL / CR / LF を含むメッセージは引き続き拒否されること。
- `tests/` / `pbt/` に obs-text を含むメッセージのデコードテストが追加されること。

## 解決方法

- `src/decoder/request.rs` / `src/decoder/response.rs` / `src/decoder/body.rs` の `String::from_utf8` による行解析を、バイト列ベースの解析に置き換える。
- `find_line` / `parse_header_line` 等のヘルパーを `&[u8]` 対応にするか、新たにバイト列版を追加する。
- `src/validate.rs` の `is_valid_field_value` 等を `&[u8]` または `char_indices()` ベースに改修する。
- 必要に応じて `RequestHead` / `ResponseHead` 内部の文字列表現を `String` から obs-text 対応の表現に変更する（影響範囲が大きい場合は別 issue に分割する）。
- `README.md` の「既知の制限事項」（obs-text は非 UTF-8 バイト列を拒否）と `AGENTS.md` の方針を統一する。
