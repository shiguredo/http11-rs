# canonical_reason() 関数を追加する

## 概要

RFC 9110 Section 15 の全ステータスコードに対応する canonical reason phrase を返す関数を `src/response.rs` に追加する。

## 動機

`Response::new(200, "OK")` のように reason phrase を毎回手書きしている。短いものは問題ないが、`"Proxy Authentication Required"` のような長い reason phrase は typo の元になる。定数的に引ける関数があるとコード補完も効いて嬉しい。

## 仕様

- `pub fn canonical_reason(status_code: u16) -> Option<&'static str>`
- RFC 9110 Section 15 の全ステータスコードを網羅する
- 未知のコードは `None` を返す
- `Response` / `ResponseHead` の型は変更しない (破壊的変更なし)

## 解決方法

- `src/response.rs` に `canonical_reason(status_code: u16) -> Option<&'static str>` を追加した
- RFC 9110 Section 15 の全 42 ステータスコードを網羅する match 式で実装した
- `src/lib.rs` で `canonical_reason` を re-export した
- `pbt/tests/prop_response.rs` に全既知コードが `Some` を返すこと、未定義コードが `None` を返すことの PBT を追加した
