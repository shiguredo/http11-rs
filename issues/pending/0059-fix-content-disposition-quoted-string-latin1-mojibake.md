# 0059 fix Content-Disposition quoted-string で UTF-8 が Latin-1 として解釈されてラウンドトリップが壊れる

Created: 2026-05-13
Model: Opus 4.7

## 概要

`fuzz_content_disposition` が `parse → to_string → parse` のラウンドトリップで `filename` パラメータの値が変化するクラッシュを検出した。
具体的には、quoted-string の中に UTF-8 マルチバイトシーケンスが含まれる場合、初回パース後の文字列が再度 Display → parse されると mojibake (文字化け) に変わる。

## 再現手順

1. `make fuzzing` (もしくは `cargo +nightly fuzz run fuzz_content_disposition`) を実行する
2. 数秒〜数十秒で `crash-b8ed6fd6c52822128d12a1862ab61068fbd31e0c` がアーティファクトに保存され `deadly signal` で停止する

入力バイト列 (hex):

```
69 6e 6c 6e 69 65 3b 66 69 6c 65 6e 61 6d 65 3d
22 61 44 44 44 44 44 44 64 74 74 61 63 68 5d 6d
65 6e 74 3b 7d 5c 2f 5c 5c 5c 5c 3b 5c 5c 5c eb
a3 a3 e9 a3 a3 5c 3b 22
```

ASCII では `inlnie;filename="aDDDDDDdttach]ment;}\/\\\\;\\\xeb\xa3\xa3\xe9\xa3\xa3\;"` に相当する (`\xeb\xa3\xa3\xe9\xa3\xa3` は UTF-8 として valid な 3-byte 2 シーケンス)。

assertion 失敗の差分:

```text
left:  Some("aDDDDDDdttach]ment;}/\\\\;\\ë££é££;")
right: Some("aDDDDDDdttach]ment;}/\\\\;\\Ã«Â£Â£Ã©Â£Â£;")
```

## 根本原因

`src/content_disposition.rs:421 parse_quoted_string` 内で、入力をバイト単位で走査して `result.push(b as char)` / `result.push(next as char)` を行っている (438, 445 行目)。

`u8 as char` は U+0000 〜 U+00FF (Latin-1) への変換になるため、UTF-8 マルチバイトシーケンス (`\xeb\xa3\xa3`) が:

- 元のバイト列を 1 文字単位 (U+00EB / U+00A3 / U+00A3) として `String` に格納する
- Rust の `String` は UTF-8 不変条件を持つため、Latin-1 として解釈された各文字は UTF-8 で 2 バイト (`\xc3\xab\xc2\xa3` など) にエンコードされて格納される
- `Display` 経由で `escape_quoted_string` がそれを UTF-8 のまま出力すると、出力バイト列は元の入力 (`\xeb\xa3\xa3`) から `\xc3\xab\xc2\xa3\xc2\xa3` に膨張する
- 再パース時に同じ `b as char` 経路を通ると、各バイトを Latin-1 として U+00C3 / U+00AB / ... に再解釈するため、`Ã«Â£` のような mojibake が生まれる

RFC 9110 Section 5.6.4 (quoted-string) は qdtext / quoted-pair に obs-text (%x80-FF) を含めることを許容しているが、その「バイト列としての保存」と「UTF-8 文字列としての解釈」の混同が原因。

## 影響

- `parse → to_string → parse` のラウンドトリップでバイト列が変化する
- 上位アプリ (リバースプロキシ等) で受信した Content-Disposition を `to_string()` して下流に再送信すると、filename の中身が壊れる
- ASCII 範囲内の quoted-string は影響を受けない

## 設計判断

修正方針として複数の選択肢があり、いずれも設計判断を伴う。pending にする理由:

1. **`filename` を `Vec<u8>` で保持する**: バイト列としての厳密な保存に切り替える破壊的変更。`filename()` の戻り値型が変わる。
2. **`obs-text` を UTF-8 シーケンスとして解釈する**: `\xeb\xa3\xa3` を U+B8E3 として扱う。RFC 9110 のバイト指向の規定とずれる。
3. **`obs-text` を含む quoted-string をエラーにする**: 後方互換性のない厳格化。
4. **Display 側で obs-text バイトを percent-encode する**: 入力と出力の表現が変わる (情報損失はない)。

`Vec<u8>` 保持に倒すなら他のヘッダーパーサー (cookie, auth 等) でも同様の議論が必要になるため、独立対応では決められない。

→ `issues/pending/` に移動する。

## 参考

- RFC 9110 Section 5.6.4 (quoted-string)
- RFC 6266 Section 4.3 (filename パラメータの解釈)
- RFC 8187 (ext-value による UTF-8 表現)
- `src/content_disposition.rs:421` parse_quoted_string
- `src/content_disposition.rs:532` escape_quoted_string
- `fuzz/fuzz_targets/fuzz_content_disposition.rs` Display ラウンドトリップ assertion
