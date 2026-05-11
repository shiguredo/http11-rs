# 0030: RequestDecoder に CONNECT 用 tunnel API を追加する

Created: 2026-05-11
Completed: 2026-05-11
Model: Opus 4.7

## 概要

`RequestDecoder` で CONNECT リクエストを受信した場合、現状は `BodyKind::None` を返して `DecodePhase::Complete` に直行する。プロキシ用途では CONNECT 受信後に 200 を返してから、クライアントから到来する後続バイトをトンネルデータとして transparent に転送する必要があるが、現状の API では実現できない。`decode_headers` の `Complete` 自動再遷移 (request.rs:488-495) によって、ヘッダー終端後にクライアントが送ってきた最初のトンネルバイトが「次の HTTP リクエスト」として parse され始めるため、HTTP Request Smuggling 様の挙動を引き起こす危険がある。

`ResponseDecoder` 側には既に `DecodePhase::Tunnel` / `BodyKind::Tunnel` / `is_tunnel()` / `take_remaining()` が実装されている (CONNECT 2xx レスポンス受信時に切り替わる)。本 issue は同等の機能を `RequestDecoder` に持たせて、proxy のサーバ側でも安全に CONNECT を扱えるようにする。

## 根拠

### RFC

- RFC 9110 Section 9.3.6: "A CONNECT request message does not have content." / "When a server responds with a 2xx (Successful) status code to a CONNECT request, the connection becomes a tunnel immediately after the header section, with the connection used as-is to convey the data of the tunnel."
- RFC 9112 Section 6.3: フレーミング判定の優先順位
- RFC 9112 Section 11.2: HTTP Request Smuggling (CWE-444)

### 攻撃シナリオ

1. 攻撃者がプロキシに `CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\nGET /admin HTTP/1.1\r\nHost: internal\r\n\r\n` を送る
2. プロキシは CONNECT ヘッダーを decode → `BodyKind::None` で `Complete` 遷移
3. プロキシが認証等を経て 200 を返そうとする間に、`decode_headers` を再度呼ぶと内側の `GET /admin` を「次のリクエスト」として parse してしまう
4. プロキシ実装によっては内側リクエストを内部ネットワークに転送 → 認証バイパス

正しくは「CONNECT ヘッダー受信後、サーバが 200 を返したらヘッダー以降のバイトを **すべて** トンネルとして転送する」必要があり、それをライブラリ API として表現する。

### 既存の `ResponseDecoder` との対称性

`ResponseDecoder` の対応 API:

- `DecodePhase::Tunnel` (CONNECT 2xx 受信後に遷移)
- `BodyKind::Tunnel` (`determine_body_kind` が返す)
- `is_tunnel() -> bool`
- `take_remaining() -> Vec<u8>`
- `decode_headers()` は Tunnel phase でエラーを返す
- `decode()` は Tunnel phase でエラーを返す

`RequestDecoder` 側にもこれらに対応する API が必要。

## 対応方針

### `src/decoder/request.rs`

- `decode_headers` 内の CONNECT メソッド判定で `BodyKind::None` を返している箇所を `BodyKind::Tunnel` に変更する
- `body_kind` ディスパッチに `BodyKind::Tunnel => self.phase = DecodePhase::Tunnel;` を追加する (現状の `unreachable!` を除去)
- `is_tunnel(&self) -> bool` メソッドを追加する
- `take_remaining(&mut self) -> Vec<u8>` メソッドを追加する (response.rs と同実装で pending リセット込み)
- `decode_headers` / `decode` を Tunnel phase で呼んだ場合のエラーを追加する (response.rs と同様)
- `decode()` の Tunnel ボディ処理 (`unreachable!`) を整理する

### `src/decoder/mod.rs`

`HttpHead` / `BodyKind` の re-export には変更なし (Tunnel は既に export 済み)。

### テスト

- `tests/test_decoder.rs`:
  - CONNECT リクエスト受信後に `is_tunnel()` が true を返すこと
  - `decode_headers` を Tunnel phase で再度呼ぶとエラーを返すこと
  - `take_remaining` で後続バイトを取り出せること
  - `reset()` で Tunnel から脱出できること
- `pbt/tests/prop_decoder/request.rs`:
  - CONNECT + 任意の後続バイト列を feed したとき、`decode_headers` 後の `take_remaining` で正確に後続バイト列が取れること

### CHANGES.md

`## develop` のメインに `[CHANGE]` として追記する (BodyKind の戻り値が変わるため後方互換性なし)。

### 破壊的変更

- 以前は CONNECT リクエストで `BodyKind::None` が返っていたが、本変更後は `BodyKind::Tunnel` が返る
- CONNECT を扱っていた既存ユーザは、`BodyKind::Tunnel` 分岐を追加する必要がある
- canary リリース中なので破壊的変更は許容範囲

## 解決方法

- `src/decoder/request.rs::decode_headers`:
  - CONNECT メソッド検出時の戻り値を `BodyKind::None` から `BodyKind::Tunnel` に変更した
  - `BodyKind::Tunnel` 分岐の `unreachable!()` を削除し `self.phase = DecodePhase::Tunnel` に置き換えた
  - `DecodePhase::Tunnel` 分岐を追加して `decode_headers` がトンネルモードでエラーを返すようにした (`ResponseDecoder::decode_headers` と対称)
- `src/decoder/request.rs::decode`:
  - `BodyKind::Tunnel` 分岐の `unreachable!()` を削除し、`InvalidData("decode() cannot be used in tunnel mode, use take_remaining() instead")` を返すように変更した
- `src/decoder/request.rs` に以下の `pub` メソッドを追加した:
  - `take_remaining(&mut self) -> Vec<u8>`: `ResponseDecoder::take_remaining` と同実装。pending を明示的にリセットしてから `mem::take(&mut self.buf)` する
  - `is_tunnel(&self) -> bool`: phase が `DecodePhase::Tunnel` かを判定
- テスト追加:
  - `tests/test_decoder.rs::test_connect_request_enters_tunnel_mode`: 旧 `test_connect_request_no_body` を改名・全面書き換え。CL / TE / 無ヘッダーの 4 パターンで `BodyKind::Tunnel` を返すこと、`is_tunnel()` が true、ヘッダー終端後のバイト列が `take_remaining()` で取得できることを検証
  - `tests/test_decoder.rs::test_connect_request_decode_headers_in_tunnel_returns_error`: CONNECT 後の `GET /admin` バイト列が次のリクエストとして parse されず、トンネルデータとして取得できることを検証 (HTTP Request Smuggling 防御)
  - `tests/test_decoder.rs::test_connect_request_reset_clears_tunnel_mode`: `reset()` で Tunnel から脱出し、通常リクエストを decode できることを検証 (CONNECT 失敗時の復帰経路)
- `CHANGES.md` の `## develop` 先頭に `[CHANGE]` エントリを追加した
