# 0043: MultipartParser の dash-boundary 直後で transport-padding CRLF 検証欠落を修正する

Created: 2026-05-12
Model: Opus 4.7

## 概要

`MultipartParser::next_part` の `Initial` ブランチで、`--<boundary>` 直後のバイトが `\r\n` (次パート区切り) でも `--` (close-delimiter) でもない場合に **そのまま `InPart` に遷移する** 経路がある。

```rust
// src/multipart.rs:355-376
if self.buffer.len() >= after_delim + 2 {
    if &self.buffer[after_delim..after_delim + 2] == b"\r\n" {
        self.pos = after_delim + 2;
        self.boundary_scan_offset = self.pos;
        self.state = ParserState::InPart;
    } else if &self.buffer[after_delim..after_delim + 2] == b"--" {
        self.state = ParserState::Finished;
        self.finished = true;
        return Ok(None);
    } else {
        // CRLF 以外の場合もパートに進む           <-- RFC 違反
        self.pos = after_delim;
        if self.buffer[self.pos..].starts_with(b"\r\n") {
            self.pos += 2;
        }
        self.boundary_scan_offset = self.pos;
        self.state = ParserState::InPart;
    }
}
```

RFC 2046 §5.1.1 の ABNF は `dash-boundary transport-padding CRLF body-part` で、`transport-padding = *LWSP-char` (SP / HTAB のみ) かつ末尾に **必須 CRLF**。`--<boundary>X` (X が CRLF / `--` 以外、特に SP/HTAB 以外の任意バイト) は不正入力として reject されるべき。

## 根拠

### 再現フロー (実機 PoC 確認済み)

入力: `--bContent-Disposition: form-data; name="a"\r\n\r\nhello\r\n--b--\r\n`

`Initial` ブランチで `--b` を index 0 で発見、`after_delim = 3`、`buffer[3..5] = "Co"` → `\r\n` でも `--` でもないため L366 else 分岐 → `pos = 3`、state = InPart → `\r\n\r\n` 発見 → ヘッダー parse 成功 → **正常 Part として返却** (name=`a`, body=`hello`)。

実機実行結果: `got Part name="a" body="hello"`、`(2nd) got None (finished=true)`。

### Parser differential (filter bypass の足場)

- フロント (WAF / proxy) が「`--<boundary>\r\n` で始まらない → preamble の一部」と判定する一方、本実装は有効なパートとして読む
- ファイルアップロード filter のバイパス経路を生む
- multipart 仕様準拠の他実装と挙動が異なるため、複数 parser 経由でデータが流れる経路で smuggling 様の不一致を起こす

### RFC 引用

```
RFC 2046 §5.1.1
delimiter := CRLF dash-boundary
close-delimiter := delimiter "--"
dash-boundary := "--" boundary
multipart-body := [preamble CRLF]
                  dash-boundary transport-padding CRLF
                  body-part *encapsulation
                  close-delimiter transport-padding
                  [CRLF epilogue]
transport-padding := *LWSP-char
```

## 影響範囲

- multipart/form-data の Content-Disposition フィルタを通過する不正入力経路
- RFC 違反入力を黙って受理するため、他 parser との挙動差が観測される
- お手本サンプル (`examples/http11_reverse_proxy`) で multipart を扱う場合の堅牢性低下

## 対応方針

### `src/multipart.rs::MultipartParser::next_part` (Initial ブランチ)

- L366-376 の else 分岐を **削除** し、代わりに `MultipartError::InvalidBoundary` (または同等) を返す
- RFC 厳密対応として「`after_delim` から `SP|HTAB` を 0 個以上スキップしたうえで `\r\n` 必須」「`--` で close-delimiter」のいずれかのみ受理
- L370-371 の「先頭の CRLF があればスキップ」は到達不能な死コードなので削除する

### テスト

- `pbt/tests/prop_multipart.rs` に「dash-boundary 直後が SP/HTAB 以外の任意バイトの場合は reject されること」を検証する PBT を追加
- `tests/test_multipart.rs` に RFC 違反入力の reject 単体テストを追加

### CHANGES.md

`## develop` に `[FIX]` として追加する。
