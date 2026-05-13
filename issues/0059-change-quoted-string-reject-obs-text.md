# 0059 change quoted-string パーサーで obs-text (0x80-FF) を reject する

Created: 2026-05-13
Model: Opus 4.7

## 概要

`fuzz_content_disposition` が `parse → to_string → parse` のラウンドトリップで `filename` パラメータの値が mojibake (文字化け) に変わるクラッシュを検出した。
根本原因は quoted-string パーサーが obs-text (0x80-FF) を `b as char` で Latin-1 として `String` に格納していること。
これにより UTF-8 マルチバイトシーケンスを 1 バイトずつ U+0080..U+00FF にマップして保存し、Display 出力時に UTF-8 として 2 バイトに膨張させる経路ができている。

本 issue では、AGENTS.md に明文化した方針 (「quoted-string パーサーは obs-text (0x80-FF) を受け付けないこと」) に従い、関連する全ての quoted-string パーサーで obs-text を `InvalidParameter` 相当のエラーで reject するよう統一する。

## 再現手順 (元バグ)

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

`String` (UTF-8 不変条件) に obs-text を 1 バイトずつ `b as char` で格納するため、`String` のバイト表現と入力のバイト表現が一致しなくなる:

- 入力バイト `\xeb` (obs-text) → `b as char` → `U+00EB` (`ë`) を `String` に push
- `String` 内部表現では `U+00EB` は UTF-8 で `\xc3\xab` の 2 バイトになる
- `Display` がそのまま出力すると `\xc3\xab` を出す
- 再パース時に `\xc3\xab` を再度 1 バイトずつ Latin-1 として扱うと `Ã«` (U+00C3 U+00AB) に化ける

RFC 9110 Section 5.5 / 5.6.4 で `obs-text = %x80-FF` は **deprecated** とマークされており、recipient は reject か SP 置換を選んでよい。RFC 6266 Section 4.3 では `filename` パラメータは US-ASCII であるべきで、非 ASCII は `filename*` (RFC 8187 ext-value) を使うのが規定の経路。

## 対象パーサー一覧 (要対応)

1. `src/validate.rs:264 is_qdtext_byte` (`u8` レベル)
2. `src/validate.rs:276 is_quoted_pair_byte` (`u8` レベル)
3. `src/content_disposition.rs:421 parse_quoted_string` (上記 validate を利用)
4. `src/auth.rs:828-847 parse_auth_params` 内の quoted-string 処理 (上記 validate を利用)
5. `src/content_type.rs:275 parse_quoted_string` (自前実装、独自に obs-text を許容している場合あり)
6. `src/expect.rs:185 parse_quoted_string` (自前実装)
7. `src/accept.rs:556 parse_quoted_string` (自前実装)
8. `src/decoder/body.rs:583-639` chunk-ext の quoted-string 処理
9. `src/cookie.rs` quoted value 経路 (parse_quoted_value テストあり、本体実装の追跡が必要)

## 修正方針

1. `is_qdtext_byte` / `is_quoted_pair_byte` を obs-text (0x80-FF) を**含まない**形に変更する
   - 現状: `HTAB / SP / %x21 / %x23-5B / %x5D-7E / obs-text`
   - 変更後: `HTAB / SP / %x21 / %x23-5B / %x5D-7E` のみ (obs-text を削除)
2. 上記 validate を共有していないパーサーは個別に obs-text reject を入れる
3. 各パーサーのテストに obs-text が reject されることを確認する単体テストを追加する
4. fuzz target の `parse → to_string → parse` ラウンドトリップ assertion はそのまま残す
   - obs-text を含む入力は parse 段階で reject されるため、ラウンドトリップに到達する入力は ASCII のみになり mojibake が起きない
5. `b as char` 経路 (Latin-1 解釈) が物理的に到達不能になるため、当該行はバグ温床として削除可能

## 変更種別

- `[CHANGE]` 後方互換のない変更
  - 旧来 obs-text を含む quoted-string を受け入れていた呼び出し側 (RFC 9110 非推奨の経路) が `InvalidParameter` 系エラーを受け取るようになる
- ブランチ名: `feature/change-quoted-string-reject-obs-text` (issue 番号は含めない)

## 影響

- ASCII 範囲のみの quoted-string は影響を受けない
- obs-text を含む Cookie / Authorization / Content-Disposition / Content-Type 等が parse エラーになる
- 非 ASCII を扱いたい場合は `filename*` (RFC 8187 ext-value) などの正規経路を使う必要がある
- 上位アプリ (リバースプロキシ等) は obs-text を含むヘッダーを fallthrough して下流に再送信できなくなる
  - HRS (HTTP Request Smuggling) の足場を消す方向に働く

## 参考

- RFC 9110 Section 5.5 (Field Values), Section 5.6.4 (Quoted Strings)
- RFC 6266 Section 4.3 (filename parameter)
- RFC 8187 (Indicating Character Encoding and Language for HTTP Header Field Parameters)
- AGENTS.md の「### RFC について」節 (本方針の根拠を明文化済み)
- fuzz アーティファクト: `fuzz/artifacts/fuzz_content_disposition/crash-b8ed6fd6c52822128d12a1862ab61068fbd31e0c`
