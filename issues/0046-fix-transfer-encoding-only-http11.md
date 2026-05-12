# 0046: Transfer-Encoding を HTTP/1.1 完全一致以外で受理しないように厳格化する

Created: 2026-05-12
Model: Opus 4.7

## 概要

`RequestDecoder::determine_body_kind` (`src/decoder/request.rs:297-309`) と `ResponseDecoder::determine_body_kind` (`src/decoder/response.rs:374-391`) の TE 禁止ガードは `version == "HTTP/1.0"` の **完全一致のみ** で、`HTTP/0.9` / `HTTP/2.0` / `HTTP/3.0` / `RTSP/1.0` / `FOO/1.0` のような version 文字列はすべて素通しで chunked として処理してしまう。

```rust
// src/decoder/request.rs:297-309
if transfer_encoding_chunked {
    if version == "HTTP/1.0" {
        return Err(Error::InvalidData(
            "Transfer-Encoding is not defined in HTTP/1.0".to_string(),
        ));
    }
    return Ok(BodyKind::Chunked);
}
```

```rust
// src/decoder/response.rs:374-391
if version == "HTTP/1.0"
    && self.headers.iter().any(|(name, _)| name.eq_ignore_ascii_case("Transfer-Encoding"))
{
    return Err(Error::InvalidData(
        "Transfer-Encoding is not defined in HTTP/1.0".to_string(),
    ));
}
```

`is_valid_protocol_version` (`src/validate.rs:85-125`) は token + DIGIT 構文だけ見る緩い検査で、RTSP / 独自プロトコルの version 文字列を構文上は許容している (本クレートの方針上 RTSP/1.0 / RTSP/2.0 も対象、AGENTS.md「RTSP/1.0 や RTSP/2.0 も利用できること」)。TE フレーミング解釈はそれと独立に「HTTP/1.1 のみ」へ厳格化する。

## 根拠

### RFC 引用

```
RFC 9112 §6.1
A client MUST NOT send a request containing Transfer-Encoding unless
it knows the server will handle HTTP/1.1 requests (or later minor
revisions); ... A server MUST NOT send a response containing Transfer-
Encoding unless the corresponding request indicates HTTP/1.1 (or
later minor revisions).
```

```
RFC 2326 §3.6 (Connections)
Note that RTSP does not (at present) support the HTTP/1.1 "chunked"
transfer coding ...

RFC 2326 §5 (General Header Fields)
Pragma, Transfer-Encoding and Upgrade headers are not defined.
```

RFC 9112 §6.1 は「HTTP/1.1 (or later minor revisions)」と書いており HTTP/1.2 の余地を残しているが、現時点で HTTP/1.2 は存在せず、本クレートは `issues/closed/0040-fix-is-keep-alive-http11-exact-match.md` で `is_keep_alive` を HTTP/1.1 完全一致に厳格化済み。同一プロジェクトでの version 判定方針を揃えるため、TE も **HTTP/1.1 完全一致のみ受理** とする。HTTP/1.2 が将来定義された場合は別 issue で再判断する。

RFC 2326 §3.6 / §5 により RTSP では TE 自体が未定義であり、`RTSP/1.0` / `RTSP/2.0` で `Transfer-Encoding: chunked` を受理することは仕様違反。

### 攻撃シナリオ (HRS)

1. 攻撃者が本実装に対し `GET /foo HTTP/0.9\r\nTransfer-Encoding: chunked\r\n\r\n<chunks>` のように、wire 形式は HTTP/1.1 互換だが version 文字列だけ `HTTP/0.9` (または `HTTP/2.0` / `FOO/1.0`) に偽装したリクエストを送信
2. 本実装の `is_valid_protocol_version` は構文上これを許容し、`determine_body_kind` の TE 禁止ガードは `HTTP/1.0` 完全一致のみのため素通り → `BodyKind::Chunked` で chunked フレーミング解釈に入る
3. 前段プロキシ / 中継アプリが version 文字列を厳格に検査して `HTTP/1.0` 互換または「サポート外」として正規化、TE は処理しない設計の場合、本実装との framing 解釈差で境界がずれ HTTP Request Smuggling 成立

### CONNECT との関係

`response.rs::determine_body_kind` は CONNECT 2xx を最優先で `BodyKind::Tunnel` にする (L365-372) ため、本 issue の修正は CONNECT 経路に影響しない。CONNECT 2xx での TE/CL 取り扱いは 0045 で別途扱う。

### 関連 issue

- 0040 (closed): `is_keep_alive` を HTTP/1.1 完全一致に厳格化。本 issue は同じ版番号判定方針を TE 経路にも適用する続編
- 0019 (closed、却下): `HttpVersion` の enum 化提案。却下済みのため本 issue では version は引き続き `String` で保持し、文字列比較で対処する
- 0033 (closed): 非 CONNECT レスポンスの TE+CL 同時 error 化。本 issue とは別経路だが HRS 防御の同根問題
- 0044 / 0045 / 0041: HRS 防御 / 公開 API 整理の同根 issue 群

## スコープ

- `determine_body_kind` の request / response 両方の TE 禁止ガードを `version != "HTTP/1.1"` の否定条件に厳格化する
- 含まない:
  - `is_valid_protocol_version` の構文検査自体 (RTSP / 独自プロトコル受信を許容する現方針を維持)
  - CONNECT 2xx の TE/CL 取り扱い (0045 で対応)
  - request 版と response 版の `determine_body_kind` の対称化 (別 issue で扱う、本 issue では非対称のまま両方修正)
  - HTTP/1.2 等の minor revision 許容 (現時点で存在しないため将来 issue で再判断)

## 対応方針

### `src/decoder/request.rs` (L297-309)

```rust
if transfer_encoding_chunked {
    if version != "HTTP/1.1" {
        return Err(Error::InvalidData(
            "Transfer-Encoding is only defined for HTTP/1.1".to_string(),
        ));
    }
    return Ok(BodyKind::Chunked);
}
```

### `src/decoder/response.rs` (L374-391)

```rust
let version = self
    .start_line
    .as_ref()
    .and_then(|sl| sl.split(' ').next())
    .unwrap_or("");
if version != "HTTP/1.1"
    && self.headers.iter().any(|(name, _)| name.eq_ignore_ascii_case("Transfer-Encoding"))
{
    return Err(Error::InvalidData(
        "Transfer-Encoding is only defined for HTTP/1.1".to_string(),
    ));
}
```

### コードコメント更新

両ファイルの該当箇所コメントを「HTTP/1.0 では未定義」から以下に書き換える:

```
// RFC 9112 Section 6.1 (HTTP/1.1 のみで定義) および RFC 2326 Section 5
// (RTSP では Transfer-Encoding は未定義) に従い、HTTP/1.1 完全一致以外で
// Transfer-Encoding が出現した場合は error 化する。HTTP/1.2 が将来定義された
// 場合は別途検討する (将来変更される可能性がある)。
```

AGENTS.md「資料を由来の機能を実装する場合は、根拠資料名、節番号、将来変更される可能性があることをコードコメントで明記する」に従い、RFC 番号と将来変更余地を明記する。

### テスト戦略

- 単体テスト (`tests/test_decoder.rs`):
  - `HTTP/1.1` + `Transfer-Encoding: chunked` → 引き続き受理 (リグレッション防止)
  - `HTTP/1.0` + `Transfer-Encoding: chunked` → 引き続き reject (既存挙動保存)
  - `HTTP/0.9` / `HTTP/2.0` / `HTTP/3.0` / `RTSP/1.0` / `RTSP/2.0` / `FOO/1.0` + `Transfer-Encoding: chunked` → reject (新規)
  - case 違い (`http/1.1` / `Http/1.1`) → reject (case-sensitive 比較)
  - 上記すべてを request 版 (`RequestDecoder`) と response 版 (`ResponseDecoder`) の両方で検証
- PBT (`pbt/tests/prop_decoder/request.rs` / `prop_decoder/response.rs`):
  - property: `forall (version != "HTTP/1.1", has_te_chunked = true) → decode_headers が Err(Error::InvalidData) を返す`
  - property: `forall (version == "HTTP/1.1", has_te_chunked = true) → decode_headers が Ok((head, BodyKind::Chunked)) を返す`

### CHANGES.md

`## develop` に `[FIX]` として追加する:

```
- [FIX] HTTP/1.1 以外の version で `Transfer-Encoding` ヘッダーを受理しないように厳格化する
  - 旧挙動では `version == "HTTP/1.0"` の完全一致のみ拒否しており、`HTTP/0.9` / `HTTP/2.0` / `HTTP/3.0` / `RTSP/1.0` / `FOO/1.0` 等で `Transfer-Encoding: chunked` を chunked フレーミングとして読み始めていた
  - RFC 9112 §6.1 (TE は HTTP/1.1 のみで定義) および RFC 2326 §5 (RTSP では Transfer-Encoding 未定義) に従い、HTTP/1.1 完全一致以外で TE が出現した場合は `Error::InvalidData` を返す
  - 0040 (`is_keep_alive` を HTTP/1.1 完全一致化) と同じ版番号判定方針に揃え、HTTP Request Smuggling (CWE-444) の足場を除去する
  - request 経路 / response 経路の両方で対称に厳格化する
  - @voluntas
```

### ブランチ

`feature/fix-transfer-encoding-only-http11` (`feature/fix-` prefix、後方互換あり: 拒否されるようになる入力は元から RFC 違反のため正規利用に影響しない、issue 番号を含まない)。

## 受け入れ基準

- `RequestDecoder::determine_body_kind` / `ResponseDecoder::determine_body_kind` の TE 禁止ガードが `version != "HTTP/1.1"` の否定条件で実装されている
- `is_valid_protocol_version` (構文検査) は変更されていない (RTSP / 独自プロトコル受信を許容する現方針を維持)
- 該当箇所のコードコメントが RFC 9112 §6.1 + RFC 2326 §5 + 将来変更余地を明示している
- `tests/test_decoder.rs` に上記テスト戦略の全ケース (HTTP/1.1 受理 + HTTP/0.9 / 1.0 / 2.0 / 3.0 / RTSP/1.0 / 2.0 / FOO/1.0 + case 違い) が request / response 両方で追加されている
- `pbt/tests/prop_decoder/request.rs` / `prop_decoder/response.rs` に PBT が追加されている
- `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` がすべて PASS
- CHANGES.md `## develop` に `[FIX]` エントリが追加されている

## RFC 参照

- RFC 9112 §6.1 (Transfer-Encoding は HTTP/1.1 のみで定義、`refs/rfc9112.txt`)
- RFC 9112 §1.3 (HTTP/1.1 family のスコープ)
- RFC 2326 §3.6 / §5 (RTSP では Transfer-Encoding 未定義、`refs/rtsp/rfc2326.txt`)
