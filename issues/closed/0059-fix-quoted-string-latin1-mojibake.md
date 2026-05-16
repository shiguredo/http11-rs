# 0059 fix quoted-string パーサーの Latin-1 mojibake を修正する

Created: 2026-05-13
Completed: 2026-05-13
Model: Opus 4.7

## 概要

`Authorization` / `Content-Disposition` の quoted-string パーサーが入力 `&str` を `as_bytes()` で 1 バイトずつ走査し `b as char` で `String` に push しているため、UTF-8 マルチバイトシーケンスを構成するバイトが独立した `U+0080..=U+00FF` の char にマップされ、`String` 内部の UTF-8 表現が原入力と乖離する Latin-1 mojibake が発生する。`parse -> to_string -> parse` のラウンドトリップで値が変化し、`fuzz_content_disposition` のラウンドトリップ assertion が失敗する。

本 issue は `parse_quoted_string` 系を char 単位走査ベースに書き換えて入力 `&str` の UTF-8 不変条件を保ち、obs-text を opaque な char として保持する。

## 方針反転と先行コミットの撤回

本 issue は先行 commit `e956716` で表明した「quoted-string パーサーで obs-text (0x80-FF) を reject する」方針を撤回し、RFC 9110 Section 5.5 の「A recipient SHOULD treat ... obs-text ... as opaque data」に従って opaque 保持に戻すものである。AGENTS.md の `### RFC について` 節も working tree で同方針に更新済み (本 issue と同一 PR で取り込む)。AGENTS.md の現状記述は obs-text の保持範囲を明示していないため、本 issue の修正に合わせて AGENTS.md にも「char 単位走査では Unicode scalar `U+0080..=U+10FFFF` (surrogate 除く) まで opaque char として保持する」を追記する。

## 再現手順

本 issue の crash 入力 (hex 64 バイト) を将来の regression seed として `fuzz/corpus/fuzz_content_disposition/regression-0059` にコミットする。再現は以下:

```
cargo +nightly fuzz run fuzz_content_disposition fuzz/corpus/fuzz_content_disposition/regression-0059
```

入力バイト列 (hex):

```
69 6e 6c 6e 69 65 3b 66 69 6c 65 6e 61 6d 65 3d
22 61 44 44 44 44 44 44 64 74 74 61 63 68 5d 6d
65 6e 74 3b 7d 5c 2f 5c 5c 5c 5c 3b 5c 5c 5c eb
a3 a3 e9 a3 a3 5c 3b 22
```

`\xeb\xa3\xa3` と `\xe9\xa3\xa3` はそれぞれ UTF-8 として valid な 3 バイトシーケンス (`U+B8E3` と `U+9D63`)。assertion 失敗の差分:

```text
left:  Some("aDDDDDDdttach]ment;}/\\\\;\\ë££é££;")
right: Some("aDDDDDDdttach]ment;}/\\\\;\\Ã«Â£Â£Ã©Â£Â£;")
```

## 根本原因

`String` の UTF-8 不変条件と `u8 as char` (Latin-1 直列化) の混同。入力 `&str` (UTF-8) に含まれる obs-text バイト `\xeb` を `b as char` で扱うと `U+00EB` (`ë`) に解釈され、`String` 内部では `\xc3\xab` の 2 バイトに展開される。`Display` 出力でこの 2 バイトがそのまま吐かれ、再 parse で `\xc3 as char` -> `U+00C3` (`Ã`)、`\xab as char` -> `U+00AB` (`«`) と再度誤解釈されて mojibake が確定する。

char 単位で valid UTF-8 を走査し、その char をそのまま `String` に push すれば `String` のバイト表現は入力と一致する (入力 `\xeb\xa3\xa3` の char は `U+B8E3` 1 個、UTF-8 で 3 バイト、原入力と同一)。

## ABNF と本実装の char 単位拡張

RFC 9110 Section 5.5 の `obs-text = %x80-FF` はオクテット (バイト) 単位の定義であり、char 単位走査の本実装はこのオクテット表現を Unicode scalar に拡張解釈する。詳細はヘルパー rustdoc (修正方針 1) を参照。前提として入力 `&str` は decoder 上流 (`src/decoder/request.rs::feed` 中の `String::from_utf8` および `src/decoder/response.rs::feed` 中の同等経路) で valid UTF-8 が保証されている。

RFC 9110 Section 5.5 が CR / LF / NUL に MUST reject or replace の二択を要求している件は、本実装では reject を選択する (Section 5.5 の `MAY retain ... safe context` の他 CTL も保守的に reject、issue 0036 の方針を維持)。

## 対象パーサー

本 issue で char 単位走査ベースに書き換える対象は **`String` を生成するために `b as char` を使っている経路のみ**:

1. `src/auth.rs::parse_auth_params` の quoted-string 経路 (`value.push(b as char)` の 2 箇所)
2. `src/content_disposition.rs::parse_quoted_string` (`result.push(b as char)` / `result.push(next as char)`)

### `b as char` を含むが本 issue で扱わない経路

`grep` で確認した `b as char` の残箇所は以下。いずれも入力が ASCII 限定または別経路で UTF-8 検証済みなので mojibake の経路にならない:

- `src/uri.rs:210, 227, 244, 1017` -- URI / percent-decode 関連。URI は RFC 3986 で US-ASCII 限定 (Section 2、`refs/rfc3986.txt` L610-624)。decoder が obs-text を reject 済 (`src/decoder/request.rs:380`)。
- `src/content_disposition.rs:513` (`encode_ext_value`) -- 送信側 ext-value 生成。`is_attr_char` (ASCII tchar 派生) で絞った byte のみ push するため非 ASCII は到達しない。
- `src/auth.rs:1002, 1005` (Digest hex デコード) -- ASCII hex digit のみ (`to_digit(16)` で hex 以外 reject)。

`is_valid_field_value` / `is_valid_field_vchar` / `is_valid_reason_phrase` (`src/validate.rs`) の byte 単位検査は据え置く (qdtext / quoted-pair の char 拡張とは独立した方針)。

### スコープ外の `parse_quoted_string`

- `src/content_type.rs::parse_quoted_string` / `src/expect.rs::parse_quoted_string` / `src/accept.rs::parse_quoted_string` -- 既に char 単位走査ベース。mojibake は発生しない。ただし qdtext / quoted-pair の文字集合検査がない (CR / LF / NUL を素通しする) ので response splitting 余地が残る。**本 issue では扱わず、後続 issue 0060 で扱う** (詳細はフォローアップ節を参照)。
- `src/decoder/body.rs::parse_quoted_string` (chunk-ext) -- `&[u8]` 走査で `String` を生成しないため mojibake 経路無し。
- `src/cookie.rs` -- RFC 6265 の独自 ABNF (cookie-octet) で本 issue のスコープ外。
- `src/decoder/request.rs:376-384` の `parts[1].bytes().any(|b| b >= 0x80)` (request-target obs-text reject、CHANGES.md L143 の `## 2026.4.0` リリース済 entry) -- request-target は quoted-string ではなく RFC 3986 で US-ASCII 限定なので、本 issue の対象外。reject ロジック本体は維持。コメント文言の修正は修正方針 5 で扱う。
- `src/content_disposition.rs::escape_quoted_string` (Display 出力) -- `for c in s.chars()` で char をそのまま push しており UTF-8 不変条件は維持される。`U+0080` 以上の char が parse 結果として保持されると Display で生 UTF-8 として出力される (送信側ポリシーは § 影響 を参照)。

## 修正方針

### 1. `src/validate.rs` に char 版ヘルパーを新設

```rust
/// qdtext char か確認 (RFC 9110 Section 5.6.4)
///
/// ABNF (bytes): qdtext = HTAB / SP / %x21 / %x23-5B / %x5D-7E / obs-text
///                obs-text = %x80-FF (RFC 9110 Section 5.5)
///
/// 本実装は valid UTF-8 `&str` を char 単位で走査するため、
/// ABNF のオクテット表現を Unicode scalar に拡張解釈し、obs-text の
/// オクテット範囲を超える Unicode scalar (`U+0100..=U+10FFFF`、
/// surrogate `U+D800..=U+DFFF` は char 型で構築不能) も opaque char
/// としてそのまま受理する。RFC 9110 Section 5.5 の
/// 「recipient SHOULD treat ... obs-text ... as opaque data」を
/// char 単位に拡張解釈したもの。
pub(crate) fn is_qdtext_char(c: char) -> bool {
    matches!(c, '\t' | ' ' | '!' | '#'..='[' | ']'..='~') || c as u32 >= 0x80
}

/// quoted-pair の右辺 char か確認 (RFC 9110 Section 5.6.4)
///
/// ABNF (bytes): quoted-pair = "\" ( HTAB / SP / VCHAR / obs-text )
///                VCHAR = %x21-7E, obs-text = %x80-FF
///
/// is_qdtext_char と同じく Unicode scalar 単位に拡張解釈する。
pub(crate) fn is_quoted_pair_char(c: char) -> bool {
    matches!(c, '\t' | ' '..='~') || c as u32 >= 0x80
}
```

`is_qdtext_byte` / `is_quoted_pair_byte` は `pub(crate)` で workspace 外には公開されていないため、関数本体と use 文を削除しても外部影響なし。

### 2. `src/auth.rs::parse_auth_params` の quoted-string 経路を書き換え

quoted-string 部分は外側の `bytes[i]` インデックス走査の中に埋め込まれているため、開く DQUOTE (`bytes[i] == b'"'`) を読んだ次のバイト位置から `&input[i..]` のサブスライスを `chars()` で走査し、`len_utf8()` で消費バイト数を積算して外側 `i` に反映する。擬似コード:

```rust
// bytes[i] == b'"' を読んだ直後 (i は開く DQUOTE の次バイトを指す)
i += 1;
let inner = &input[i..];
let mut iter = inner.chars();
let mut value = String::new();
let mut consumed: usize = 0; // value 部分 + 閉じ DQUOTE が占めるバイト数
let mut closed = false;
while let Some(c) = iter.next() {
    if c == '"' {
        consumed += 1; // 閉じ DQUOTE は ASCII 1 バイト
        closed = true;
        break;
    } else if c == '\\' {
        consumed += 1; // バックスラッシュは ASCII 1 バイト
        // None の場合の早期 return では consumed は捨てられるが、
        // 外側 i を更新しないため実害は無い。
        let next_c = iter.next().ok_or(AuthError::InvalidParameter)?;
        if !is_quoted_pair_char(next_c) {
            return Err(AuthError::InvalidParameter);
        }
        consumed += next_c.len_utf8();
        value.push(next_c);
    } else {
        if !is_qdtext_char(c) {
            return Err(AuthError::InvalidParameter);
        }
        consumed += c.len_utf8();
        value.push(c);
    }
}
if !closed {
    return Err(AuthError::InvalidParameter);
}
i += consumed; // i は閉じ DQUOTE の次のバイト位置を指す
// 外側の OWS スキップ (auth.rs L878 付近) と , 整合 (L884 付近) は不変。
```

### 3. `src/content_disposition.rs::parse_quoted_string` を書き換え

`parse_quoted_string` は両端 DQUOTE を除いた中身 (`&str`) を受け取る関数なので、外側との offset 整合は不要。`chars()` ベースに書き換える:

```rust
fn parse_quoted_string(s: &str) -> Result<String, ContentDispositionError> {
    let mut result = String::with_capacity(s.len());
    let mut iter = s.chars();
    while let Some(c) = iter.next() {
        if c == '\\' {
            let next = iter.next().ok_or(ContentDispositionError::InvalidParameter)?;
            if !is_quoted_pair_char(next) {
                return Err(ContentDispositionError::InvalidParameter);
            }
            result.push(next);
        } else {
            if !is_qdtext_char(c) {
                return Err(ContentDispositionError::InvalidParameter);
            }
            result.push(c);
        }
    }
    Ok(result)
}
```

`use crate::validate::{is_qdtext_byte, is_quoted_pair_byte};` は `use crate::validate::{is_qdtext_char, is_quoted_pair_char};` に置換する (`src/auth.rs:36` と `src/content_disposition.rs:26` の 2 箇所)。

### 4. 旧 `is_qdtext_byte` / `is_quoted_pair_byte` を削除

`grep -rn` で確認: 呼び出し元は `src/auth.rs:36` と `src/content_disposition.rs:26` の use 文と、それぞれ 2 箇所の関数本体のみ。`pub(crate)` 可視性のため workspace 外からは参照不能。char 版に置き換えた後は呼び出し元が無くなるため、関数本体と use 文を両方削除する。`decoder/body.rs::is_qdtext` は別ローカル関数で `&[u8]` 用途、影響なし。

### 5. `src/decoder/request.rs:376-384` のコメント文言修正

reject 方針撤回に伴い「validate.rs 側の obs-text 許容撤去は別 issue で対応する暫定措置」を「request-target は RFC 3986 Section 2 で US-ASCII 限定であり、decoder 側でも obs-text を reject する」に書き換える。reject ロジック本体 (`parts[1].bytes().any(|b| b >= 0x80)`) は維持。

### 6. fuzz_auth.rs に Digest 系のラウンドトリップ assertion を追加

`fuzz/fuzz_targets/fuzz_auth.rs` の `DigestChallenge::parse` / `DigestAuth::parse` 経路は現状 Display reparse の戻り値を `let _` で破棄しているため、`parse_auth_params` 内の Latin-1 mojibake を fuzz が検知できない。修正後の挙動を fuzz でも保護するため、以下のラウンドトリップ assertion を追加する。`DigestChallenge` / `DigestAuth` は構造上 `params: Vec<(String, String)>` の 1 フィールドのみを持ち、専用 getter は限られているため、専用 getter と `param("name")` 汎用アクセサを併用する:

- `DigestChallenge`: 専用 getter `realm()` / `nonce()`、および `param("opaque")` / `param("domain")` / `param("qop")` / `param("algorithm")` / `param("userhash")` / `param("stale")` を Display reparse 前後で equal 比較する
- `DigestAuth`: 専用 getter `username()` / `username_decoded()` / `realm()` / `nonce()` / `uri()` / `response()`、および `param("opaque")` / `param("cnonce")` / `param("nc")` / `param("qop")` / `param("algorithm")` を Display reparse 前後で equal 比較する

`Option<&str>` / `Option<String>` は `is_none()` 状態も含めて equal 比較する。`BasicAuth` / `WwwAuthenticate` に既存の assertion がある場合はそれと整合させる。

実装着手前に「現状の `fuzz/corpus/fuzz_auth/` を Digest assertion 追加版の harness で 1 回 replay し既存 crash が無いこと」を確認する (Latin-1 mojibake 起因の latent crash 検出のため)。発見した場合は本 issue の修正範囲で resolve するか、別 crash として `fuzz/artifacts/` に保存する。

## 変更種別

`[FIX]` バグ修正

- 旧来 mojibake していた obs-text + UTF-8 マルチバイト入力が opaque な char として保持されるようになる
- 寛容化方向で API 互換は破壊しない (旧来 mojibake 出力に依存していた外部利用者は乖離する可能性があるが、これはバグの是正)

ブランチ名: `feature/fix-quoted-string-latin1-mojibake` (CLAUDE.md 規約準拠、issue 番号は含めない)

## CHANGES.md エントリ案

未リリースの `## develop` に `[FIX]` 種別を新設し、`### misc` の上に挿入する (規約: UPDATE -> ADD -> CHANGE -> FIX -> misc の順):

```
- [FIX] `Authorization` / `Content-Disposition` の quoted-string パースで obs-text を含む UTF-8 値の Latin-1 mojibake を修正する
  - 旧実装は入力 `&str` を `as_bytes()` で 1 バイトずつ走査し `b as char` で `String` に push していたため、UTF-8 マルチバイトシーケンスが `U+0080..=U+00FF` にマップされ Display 出力で別バイトに展開、ラウンドトリップで mojibake していた
  - char 単位走査に書き換え、入力 `&str` の UTF-8 不変条件を保つ
  - obs-text は RFC 9110 Section 5.5 の「recipient SHOULD treat obs-text as opaque data」に従い opaque な char として保持する (reject しない)。CR / LF / NUL の reject は char 版ヘルパー `is_qdtext_char` / `is_quoted_pair_char` で等価に維持する
  - issue 0036 で導入した `is_qdtext_byte` / `is_quoted_pair_byte` (`pub(crate)`、2026.4.0 リリース済) を char 版に置き換え本体を削除する
  - @voluntas
```

## 受け入れ基準

- [ ] `fuzz/corpus/fuzz_content_disposition/regression-0059` を新設 (上記 hex バイト列の生バイト)
- [ ] `cargo +nightly fuzz run fuzz_content_disposition fuzz/corpus/fuzz_content_disposition/regression-0059` が assertion 失敗せず終了する
- [ ] `cargo +nightly fuzz run fuzz_content_disposition -- -max_total_time=60` で新規 crash が出ない
- [ ] `cargo +nightly fuzz run fuzz_auth -- -max_total_time=60` で新規 crash が出ない (Digest 系のラウンドトリップ assertion 追加後)
- [ ] Digest 系 assertion 追加版の harness で現状の `fuzz/corpus/fuzz_auth/` を 1 回 replay し既存 crash が無いこと
- [ ] `make fmt && make clippy && make check && make test` が pass
- [ ] `cargo test -p pbt --test prop_content_disposition` / `cargo test -p pbt --test prop_auth` が pass
- [ ] BMP 内の 2 バイト UTF-8 (`U+00E9` = `é`)、3 バイト UTF-8 (`U+65E5` = `日`)、BMP 末尾 (`U+D7FF`)、surrogate 直後 (`U+E000`)、4 バイト UTF-8 最大 (`U+10FFFF`) を含む `filename` がパース成功する単体テストを `tests/test_content_disposition.rs` に追加
- [ ] 同等の char を含む `Authorization: Basic realm="..."` がパース成功する単体テストを `tests/test_auth.rs` に追加
- [ ] CR / LF / NUL を含む quoted-string / quoted-pair は引き続き `InvalidParameter` で reject される (issue 0036 のリグレッション防止、既存テストが pass すること)
- [ ] AGENTS.md `### RFC について` 節に「char 単位走査では Unicode scalar `U+0080..=U+10FFFF` (surrogate 除く) まで opaque char として保持する」を追記する
- [ ] CHANGES.md `## develop` の `### misc` の上に `[FIX]` セクションを新設し、上記エントリを追加する

## テスト

### PBT 拡張

`pbt/tests/prop_content_disposition.rs::qdtext_char` の現状:

```rust
fn qdtext_char() -> impl Strategy<Value = char> {
    prop_oneof![
        Just('\t'),
        Just(' '),
        Just('!'),
        prop::char::range('#', '['),  // 0x23-0x5B
        prop::char::range(']', '~'),  // 0x5D-0x7E
    ]
}
```

これを obs-text 拡張 (Unicode scalar 全域) を含む形に置換:

```rust
fn qdtext_char() -> impl Strategy<Value = char> {
    prop_oneof![
        Just('\t'),
        Just(' '),
        Just('!'),
        prop::char::range('#', '['),
        prop::char::range(']', '~'),
        // obs-text を Unicode scalar として opaque 保持。
        // surrogate (`U+D800..=U+DFFF`) は char 型で構築不能なので
        // `prop::char::range` が surrogate 範囲を内包しても安全だが、
        // shrink バイアスを surrogate 跨ぎで歪めないため二分割する。
        prop::char::range('\u{80}', '\u{D7FF}'),
        prop::char::range('\u{E000}', '\u{10FFFF}'),
    ]
}
```

`pbt/tests/prop_auth.rs` には `qdtext_char` 相当の strategy が無いので、同等の strategy を新設し `realm` などの auth-param 値 (quoted-string) のラウンドトリップを追加する。

### Fuzzing

- `fuzz/fuzz_targets/fuzz_content_disposition.rs` の `parse -> to_string -> parse` ラウンドトリップ assertion (`cd.filename() == reparsed.filename()` 等) は **変更しない**。本 issue の修正で同 assertion がそのまま pass するのが期待挙動。
- `fuzz/corpus/fuzz_content_disposition/regression-0059` に上記 hex バイト列の生バイトをコミットして regression seed として残す。

## 影響

- obs-text を含む UTF-8 char 値の `Authorization` / `Content-Disposition` ヘッダーが parse 可能になる (寛容化、API 互換破壊なし)
- リバースプロキシ等で受信した obs-text 入りヘッダーを下流に `Display` 経由で fallthrough できるようになる (mojibake せず原入力のバイト列を保つ)
- 送信側 (Display 出力) の挙動: parse 結果として保持された `U+0080` 以上の char はそのまま生 UTF-8 として Display に出力される。RFC 9110 Section 5.5 は「Specifications for newly defined fields SHOULD limit their values to visible US-ASCII octets (VCHAR), SP, and HTAB」と仕様定義者向けに US-ASCII を SHOULD で要請するが、本 issue は受信側 (Postel 原則「受信は寛容」) のスコープであり、送信側で US-ASCII を保証する責務は呼出側にある。具体的には:
  - `ContentDisposition::with_filename` (`src/content_disposition.rs:292-295`) は infallible で String を受け取りバリデーション無し。obs-text 入り値を渡すと Display で生 UTF-8 を出力する。送信側で US-ASCII を保証するには `with_filename_ext` (RFC 8187 ext-value、UTF-8 percent-encoded) を使うか、呼出側で ASCII に正規化する必要がある。
- 旧来 mojibake していた Display 出力に依存する外部利用者は乖離するが、これは元バグの是正。

## フォローアップ

本 issue の修正 PR の merge とは独立して、以下を別コミット / 別 PR で実施する:

- `issues/SEQUENCE` を `0061` に更新し、`0060-fix-quoted-string-charset-validation.md` を起票する。対象は `src/content_type.rs` / `src/expect.rs` / `src/accept.rs` の `parse_quoted_string` への qdtext / quoted-pair 文字集合検査 (本 issue で導入する char 版ヘルパーを再利用)。

## 参考

- RFC 9110 Section 5.5 (Field Values): `obs-text = %x80-FF` (バイト範囲)、recipient SHOULD treat obs-text as opaque data、CR / LF / NUL は MUST reject or replace with SP の二択 (本実装は reject を選択)
- RFC 9110 Section 5.6.4 (Quoted Strings): qdtext / quoted-pair の ABNF (Section 5.5 の obs-text を参照)
- RFC 3986 Section 2 (Characters): URI は US-ASCII 文字集合に基づく (request-target が obs-text を含まない根拠)
- RFC 8187 Section 3.2 (ext-value): 送信側で非 ASCII を扱う際の RFC 規定 (本 issue 受信側のスコープ外)
- RFC 6266 Appendix D (Advice on Generating Content-Disposition Header Fields): 「Include a `filename*` parameter where the desired filename cannot be expressed faithfully using the `filename` form」(送信側に向けた助言)。Section 4.3 の recipient 向け SHOULD (`SHOULD pick "filename*"`) と併せて参照
- 関連先行 issue: `issues/closed/0036-fix-quoted-string-quoted-pair-ctl-rejection.md` (CR / LF / NUL reject 経路を `is_qdtext_byte` / `is_quoted_pair_byte` で導入、本 issue は同経路を char 版に移植)

## 解決方法

1. `src/validate.rs` に char 版ヘルパー `is_qdtext_char` / `is_quoted_pair_char` を新設し、旧 byte 版 `is_qdtext_byte` / `is_quoted_pair_byte` を削除した。Unicode scalar `U+0080..=U+10FFFF` (surrogate 除く) まで opaque char として受理する。
2. `src/auth.rs::parse_auth_params` の quoted-string 経路を `&input[i..]` の `chars()` 走査ベースに書き換え、`len_utf8()` で消費バイト数を外側 `i` に反映するようにした。
3. `src/content_disposition.rs::parse_quoted_string` を `chars()` ベースに書き換え、char をそのまま `String` に push するようにした。
4. `src/decoder/request.rs:376-384` のコメントを「RFC 3986 で US-ASCII 限定」根拠に書き換えた (reject ロジック本体は維持)。
5. `fuzz/fuzz_targets/fuzz_auth.rs` に `DigestChallenge` / `DigestAuth` の Display reparse 前後の equal 比較を追加した。
6. `fuzz/corpus/fuzz_content_disposition/regression-0059` を新設 (issue 記載の 56 バイト hex 列)。
7. PBT: `pbt/tests/prop_content_disposition.rs::qdtext_char` を Unicode scalar 拡張に置換、`pbt/tests/prop_auth.rs` に `qdtext_char` / `qdtext_realm` strategy と `WwwAuthenticate` / `DigestChallenge` の obs-text ラウンドトリップを追加した。
8. 単体テスト: `tests/test_content_disposition.rs` と `tests/test_auth.rs` に BMP 内 2 バイト / 3 バイト UTF-8 / `U+D7FF` / `U+E000` / `U+10FFFF` を含む値のラウンドトリップと、CR / LF / NUL の reject (qdtext / quoted-pair 両方) を追加した。`test_content_disposition_regression_0059` で issue 記載の hex 列が `aDDDDDDdttach]ment;}/\\;\<U+B8E3><U+98E3>;` として正しくパースされラウンドトリップで mojibake しないことを検証した (issue 文中の `U+9D63` は誤記、実際は `\xe9\xa3\xa3` = `U+98E3`)。
9. `AGENTS.md` の `### RFC について` 節に「char 単位走査では Unicode scalar `U+0080..=U+10FFFF` (surrogate 除く) まで opaque char として保持する」を追記した。
10. `CHANGES.md` `## develop` の `### misc` の上に `[FIX]` エントリを追加した。

### 検証

- `make fmt && make clippy && make check && make test`: pass
- `cargo +nightly fuzz run fuzz_content_disposition fuzz/corpus/fuzz_content_disposition/regression-0059`: assertion 失敗せず終了
- `cargo +nightly fuzz run fuzz_content_disposition -- -max_total_time=60`: 4816950 runs / 61 秒 crash 無し
- `cargo +nightly fuzz run fuzz_auth fuzz/corpus/fuzz_auth/ -- -runs=0`: corpus 10763 files 一括 replay で crash 無し
- `cargo +nightly fuzz run fuzz_auth -- -max_total_time=60`: 9958917 runs / 61 秒 crash 無し
