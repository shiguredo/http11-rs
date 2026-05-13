# 0045: ResponseDecoder の CONNECT 2xx で Transfer-Encoding / Content-Length を ResponseHead から除去する

Created: 2026-05-12
Completed: 2026-05-12
Model: Opus 4.7

## 概要

`ResponseDecoder::determine_body_kind` (`src/decoder/response.rs:344-372`) は CONNECT メソッドへの 2xx レスポンスを受信したとき即 `BodyKind::Tunnel` を返し、フレーミング解釈上は RFC 9110 §9.3.6 の MUST ignore に準拠している。しかし `ResponseHead.headers` には `Transfer-Encoding` / `Content-Length` の元値がそのまま残置されるため、上位アプリ (reverse proxy 等) が `head.get_header(...)` / `head.content_length()` / `head.is_chunked()` 経由でその値を観測し下流に再生成すると、parser differential 経由で HTTP Request Smuggling の足場になる。

本 issue は MUST ignore を「物理消去」で実装する: `BodyKind::Tunnel` 遷移直前に `ResponseHead` の `headers` から `Transfer-Encoding` / `Content-Length` を除去する (silent drop)。

## 根拠

### RFC 9110 §9.3.6 引用

```
A server MUST NOT send any Transfer-Encoding or Content-Length header
fields in a 2xx (Successful) response to CONNECT. A client MUST
ignore any Content-Length or Transfer-Encoding header fields received
in a successful response to CONNECT.
```

「MUST ignore」は MUST レベルの強い要求で、SHOULD reject と両立しない。本 issue は **error 化しない**。代わりに ResponseHead から物理的に取り除くことで、MUST ignore を「上位層が値を観測できない」形で保証する。

### 既存実装との関係

- `tests/test_decoder.rs::test_connect_2xx_ignores_body_headers` (L143-173) は「CONNECT 2xx + CL/TE 受信 → `BodyKind::Tunnel` 遷移」を検証する既存テストで、本 issue 対応後も Tunnel 遷移自体は維持される。本 issue で追加すべきは「`ResponseHead.headers` から CL/TE が消えていること」のアサーション
- `issues/closed/0033-fix-response-content-length-transfer-encoding-conflict.md` は CONNECT **以外** のレスポンスでの TE+CL 同時受信を error 化した。RFC 9112 §6.3 の precedence では item 2 (CONNECT 2xx → tunnel + MUST ignore) が item 3 (TE+CL conflict) より先に短絡するため、本 issue と 0033 は独立して両立する
- `issues/closed/0030-change-request-decoder-connect-tunnel-api.md` で CONNECT 受信側に `BodyKind::Tunnel` と `take_remaining` API を導入済み。本 issue はその headers 観測層の MUST ignore 補強
- リクエスト側 (`src/decoder/request.rs:455-464`) は `2026.1.1` で「CONNECT リクエストの CL/TE は MUST NOT がないため reject しない」と確定済み。本 issue ではリクエスト側に手を入れない (過去の方針逆行を避ける)

### Smuggling シナリオ

1. 攻撃者制御の origin が `HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\n<5 byte の smuggled response>...` を返す
2. 本実装 decoder は MUST ignore に従い `BodyKind::Tunnel` を返す (CL は無視)
3. しかし上位アプリ (reverse proxy) が `head.content_length()` 経由で `Some(5)` を取得し、下流に `Content-Length: 5` を再生成
4. 下流クライアントは CL=5 で body を読んで本物のレスポンスを終え、続く smuggled response を「次のレスポンス」として解釈 → HTTP Response Smuggling 成立

物理消去で 3 のステップを断つ。

## スコープ

- `ResponseDecoder` 受信側で CONNECT 2xx 遷移時に `ResponseHead.headers` から `Transfer-Encoding` / `Content-Length` を除去する
- 含まない:
  - CONNECT 非 2xx (`4xx` / `5xx` 等): 通常レスポンスとして TE/CL を解釈する従来挙動を維持 (`determine_body_kind` の CONNECT + 2xx 分岐外を経由するため自然に対象外)
  - `Connection` / `Upgrade` / `Trailer` 等の他 hop-by-hop ヘッダー: 別 issue で扱う
  - CONNECT リクエスト側の CL/TE 扱い: `2026.1.1` で確定済みの方針を維持
  - encoder 側 (`src/encoder.rs`): エンコーダはリクエストメソッドを知らないため、CONNECT 2xx の TE/CL チェックは呼出側責務。本 issue では触らない
  - RTSP context: RTSP に CONNECT は存在しないため適用なし

## 対応方針

### `src/decoder/response.rs::determine_body_kind`

CONNECT + 2xx 分岐に入る前段または直後で `self.headers` から `Transfer-Encoding` / `Content-Length` を除去する:

```rust
if self
    .request_method
    .as_deref()
    .is_some_and(|m| m == "CONNECT")
    && (200..300).contains(&status_code)
{
    self.headers.retain(|(name, _)| {
        !name.eq_ignore_ascii_case("Transfer-Encoding")
            && !name.eq_ignore_ascii_case("Content-Length")
    });
    return Ok(BodyKind::Tunnel);
}
```

`headers` フィールドの所有権と `ResponseHead` 構築タイミングの兼ね合いで `from_validated_parts` の段階で除去するほうがクリーンなら、そちらでも可。呼出側に観測される `ResponseHead` 時点で CL/TE が消えていれば実装方式は問わない。

### テスト

- 単体テスト (`tests/test_decoder.rs`):
  - `test_connect_2xx_ignores_body_headers` を拡張: 既存の `BodyKind::Tunnel` アサーションに加え、`head.get_header("Content-Length") == None` / `head.get_header("Transfer-Encoding") == None` / `head.content_length() == Ok(None)` / `head.is_chunked() == false` を verify
  - CONNECT 非 2xx (例: 502 with `Content-Length: 5`) では従来通り CL が `ResponseHead.headers` に残ることを確認する単体テストを追加
- PBT (`pbt/tests/prop_decoder/response.rs`):
  - property: `forall (status ∈ 200..300, te: Option<String>, cl: Option<u64>, method = "CONNECT") → decode_headers が Ok((head, BodyKind::Tunnel)) を返し、head.get_header("Transfer-Encoding") == None ∧ head.get_header("Content-Length") == None`
  - property: `forall (status ∈ 100..200 or status ∈ 300..600, ..., method = "CONNECT") → CL/TE が head に残る`

### CHANGES.md

`## develop` に `[CHANGE]` として追加する (`ResponseHead.headers` 観測挙動の後方互換破壊):

```
- [CHANGE] `ResponseDecoder` が CONNECT 2xx レスポンス受信時に `ResponseHead.headers` から `Transfer-Encoding` / `Content-Length` を除去するように変更する
  - RFC 9110 §9.3.6 「A client MUST ignore any Content-Length or Transfer-Encoding header fields received in a successful response to CONNECT」を物理消去で実装する
  - 旧実装は `BodyKind::Tunnel` を返す一方で `ResponseHead.headers` には CL/TE の元値が残置されており、上位アプリ (reverse proxy 等) が `head.content_length()` 経由で値を観測して下流に再生成すると HTTP Response Smuggling の足場となる経路を持っていた
  - 上位アプリが `head.get_header("Content-Length")` 等で CL/TE を取得していた経路は本変更で `None` を返すようになる
  - @voluntas
```

### ブランチ

`feature/change-response-decoder-connect-2xx-drop-te-cl` (`feature/change-` prefix、後方互換のない変更、issue 番号を含まない)。

## 受け入れ基準

- `ResponseDecoder` が CONNECT 2xx 受信時に `ResponseHead.headers` から `Transfer-Encoding` / `Content-Length` を除去している
- `head.get_header("Content-Length")` / `head.get_header("Transfer-Encoding")` / `head.content_length()` / `head.is_chunked()` のすべてで CL/TE が観測できない
- `tests/test_decoder.rs::test_connect_2xx_ignores_body_headers` が headers 観測層まで verify するように拡張されている
- CONNECT 非 2xx レスポンスでは従来通り CL/TE が `ResponseHead.headers` に残ることが単体テストで確認されている
- `pbt/tests/prop_decoder/response.rs` に CONNECT 2xx で CL/TE が head から消える性質の PBT が追加されている
- `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` がすべて PASS
- CHANGES.md `## develop` に `[CHANGE]` エントリが追加されている

## 関連 issue

- 0030 (closed): CONNECT 受信側に `BodyKind::Tunnel` と `take_remaining` API を導入。本 issue はその headers 観測層の MUST ignore 補強
- 0033 (closed): 非 CONNECT レスポンスの TE+CL 同時 error 化。本 issue とは RFC §6.3 の precedence で独立
- 0044: `HttpHead::content_length` の厳格パース化。本 issue で headers から CL を除去すれば、0044 修正後の `content_length()` も `Ok(None)` を返すようになり MUST ignore が完全実装される
- 0046: HTTP/1.1 完全一致以外で TE reject。HRS 防御の同根問題

## RFC 参照

- RFC 9110 §9.3.6 (CONNECT への 2xx で TE/CL は MUST ignore)
- RFC 9112 §6.3 item 2 (CONNECT 2xx → tunnel、framing 解釈に TE/CL を使わない)

すべて `refs/rfc9110.txt` / `refs/rfc9112.txt` で参照可能。

## 解決方法

- `src/decoder/response.rs::determine_body_kind` のシグネチャを `&self` → `&mut self` に変更し、CONNECT + 2xx 判定後の `BodyKind::Tunnel` 返却直前で `self.headers.retain(...)` を呼び `Transfer-Encoding` / `Content-Length` を物理消去するよう変更した
- `tests/test_decoder.rs::test_connect_2xx_ignores_body_headers` を拡張し、`head.get_header("Transfer-Encoding") / get_header("Content-Length") / is_chunked() / content_length()` が全て None / false を返すこと、および CONNECT 非 2xx (502) では従来通り CL が残ることを assertion 化した
- `pbt/tests/prop_decoder/response.rs` に `prop_connect_2xx_drops_te_cl_from_head` / `prop_connect_non_2xx_keeps_cl_in_head` を追加した
- `CHANGES.md` の `## develop` に `[CHANGE]` エントリを追加した
