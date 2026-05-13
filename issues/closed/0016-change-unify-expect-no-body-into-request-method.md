# 0016: ResponseDecoder の expect_no_body を request_method に統合する

Created: 2026-05-06
Completed: 2026-05-06
Model: Opus 4.7

## 概要

`ResponseDecoder` の HEAD レスポンス指定 (`expect_no_body: bool` /
`set_expect_no_body()`) を撤去し、CONNECT トンネル判定用の `request_method:
Option<String>` / `set_request_method()` 一本に集約する。HEAD レスポンスとして
扱いたい場合は `set_request_method("HEAD")` を呼ぶ。

破壊的変更。

## 根拠

`ResponseDecoder` は RFC 9112 Section 6.3 のメッセージ長決定規則を実装する。
このうち「リクエストメソッドに依存する分岐」は本来同じ情報源 (= レスポンスの
元になったリクエストのメソッド) を参照すべきだが、現状は 2 つの別フィールド
で重複管理されている。

- CONNECT 2xx → トンネル判定: `request_method` を `determine_body_kind` 内で
  `method == "CONNECT"` と比較
- HEAD レスポンス → ボディなし判定: `expect_no_body: bool` を判定

### 問題 1: 同じ情報を別々のフィールドで管理している

「このレスポンスは何のリクエストへの応答か」という同一の情報が CONNECT 用と
HEAD 用で別フィールドに分かれており、`reset()` でも両方を個別にクリアしている。
RFC 9112 Section 6.3 item 1 は HEAD/1xx/204/304 を並列に扱っており、

> Any response to a HEAD request and any response with a 1xx (Informational),
> 204 (No Content), or 304 (Not Modified) status code is always terminated by
> the first empty line after the header section, regardless of the header
> fields present in the message.

「HEAD レスポンスはヘッダフィールドの内容に関わらずヘッダ終了で終わる」と
規定している。CONNECT 2xx も同 Section 6.3 item 2 で「リクエストメソッドに
依存する分岐」として並んでおり、実装でも同じ抽象 (= 元リクエストのメソッド) に
寄せるべき。

### 問題 2: 利用側 API が一貫していない

CONNECT は `set_request_method("CONNECT")`、HEAD は `set_expect_no_body(true)`
と別々の API になっている。「リクエストメソッドに応じてレスポンスデコーダーを
セットアップする」という同じ目的の操作なのに、メソッドごとに呼ぶ関数が違う。

### 問題 3: リセットの対称性が崩れている (状態漏れバグ)

`expect_no_body` は `reset()` だけでなく、`decode_headers()` の
`DecodePhase::Complete` 遷移時と `decode()` 完了時の Keep-Alive リセットでも
`false` にクリアされる。一方 `request_method` は `reset()` でしかクリアされない。

このため Keep-Alive 接続で同一デコーダーを使い回す場合、以下のシナリオで
バグが発生する:

1. `set_request_method("CONNECT")` を呼ぶ
2. 上流が `HTTP/1.1 400 Bad Request\r\nContent-Length: 5\r\n\r\nerror` を返す
   → CONNECT 2xx ではないので通常の `ContentLength(5)` として処理され、完了
3. `request_method` は `"CONNECT"` のまま
4. 次のレスポンスが `HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello`
   → status が 2xx かつ `request_method == "CONNECT"` なので、誤って
   `BodyKind::Tunnel` と判定される

`request_method` 一本に統合する際に、`Complete` 遷移時と `decode()` 完了時にも
`request_method = None` を追加することで、このバグを修正する。

### 問題 4: determine_body_kind の判定順序が RFC 9112 Section 6.3 の優先順位と一致していない

RFC 9112 Section 6.3 の明示的な優先順位 ("in order of precedence"):

1. HEAD / 1xx / 204 / 304 → ボディなし (item 1)
2. CONNECT 2xx → トンネル (item 2)
3. TE と CL が両方ある場合、TE が優先 (item 3)
4. TE あり: chunked が最後 → chunked、そうでなければ close-delimited (item 4)
5. 無効な CL → エラー (item 5)
6. 有効な CL → 固定長 (item 6)
7. リクエストで上記のいずれも該当しない → ボディ長ゼロ (item 7)
8. レスポンスで上記のいずれも該当しない → close-delimited (item 8)

しかし現状の `determine_body_kind` は「HTTP/1.0 + TE チェック」と「CONNECT」
を「HEAD / 1xx / 204 / 304」より先に判定している。この順序では以下の 2 つの
非準拠が発生する:

- HEAD + HTTP/1.0 + TE のレスポンスが誤ってエラーになる
  (item 1 の "regardless of the header fields present" に反する)
- CONNECT + 204 が `BodyKind::Tunnel` と判定される
  (item 1 が item 2 より優先されるため、204 は常にボディなしにすべき)

本 issue で RFC 通りの順序に修正する。この修正自体が CONNECT + 204 の挙動を
`Tunnel` → `None` に変更する (破壊的変更だが RFC 準拠への修正)。

## 対応方針

### src/decoder/response.rs

- フィールド `expect_no_body: bool` と API `pub fn set_expect_no_body(&mut self, bool)` を撤去する
- 4 箇所のコンストラクタ (`new` / `with_limits` / `with_decompressor` / `with_decompressor_and_limits`) の `expect_no_body: false` 初期化を削除する
- `request_method` フィールドの doc コメント (現状: `リクエストメソッド (CONNECT トンネル判定用)`) を「HEAD/CONNECT 判定用」に更新する
- `set_request_method` の doc コメント (現状: CONNECT 専用の説明) を、
  `"HEAD"` を渡すとボディなし扱い、`"CONNECT"` への 2xx でトンネルモード、と
  両方を担う旨に更新する
- `determine_body_kind` を RFC 9112 Section 6.3 の優先順位に従った以下のロジックに
  書き換える。各判定に対応する RFC 節番号をコードコメントに残す (AGENTS.md「資料由来の
  機能を実装する場合は、根拠資料名、節番号、将来変更される可能性があることをコード
  コメントで明記すること」に従う)。具体的な書き方は実装裁量。既存コードの let-chain
  スタイルに合わせると差分が読みやすい:

  - RFC 9112 Section 6.3 item 1: `request_method == "HEAD"` または
    `!status_has_body(status_code)` なら `BodyKind::None`
    (HEAD / 1xx / 204 / 304 はヘッダフィールドの内容に関わらずヘッダ終了で終わる。
    RFC 9112 Section 6.3: "in order of precedence" により item 1 が最も優先される。
    このため CONNECT + 204 の挙動が従来の `Tunnel` から `None` に変わるが、
    これは RFC 準拠への修正である。
    205 は `status_has_body` が `true` を返すためここではマッチせず、後続の TE/CL
    解析に進む (RFC 9110 Section 15.3.6: 送信者制約のみ。現状維持))
  - RFC 9112 Section 6.3 item 2: `request_method == "CONNECT"` かつ status が
    2xx なら `BodyKind::Tunnel`
    (item 1 で 1xx/204/304 は既に返っているため、ここに到達するのは status が
    200-203, 205-299 でかつ `request_method == "CONNECT"` の場合のみ。
    RFC 9110 Section 9.3.6: CONNECT への 2xx はヘッダ終了直後にトンネルモードへ
    切り替わる。RFC 9110 Section 9.1: メソッドトークンは case-sensitive)
  - RFC 9112 Section 6.1: HTTP/1.0 + Transfer-Encoding は framing fault
    (item 1 で HEAD/1xx/204/304 は既に返っているため、このチェックに到達するのは
    ボディが存在しうるレスポンスのみ)
  - RFC 9112 Section 6.3 item 3〜8: TE/CL 解析
    (`resolve_body_headers_for_response` に委譲。chunked → Chunked、
    非 chunked → CloseDelimited、CL → ContentLength、どちらもなし → CloseDelimited
    (item 8))
- `decode_headers()` の `DecodePhase::Complete` 遷移時に
  `self.request_method = None` を追加する (`expect_no_body = false` は撤去)
- `decode()` 完了時の Keep-Alive リセット処理に `self.request_method = None` を
  追加する (`expect_no_body = false` は撤去)
- `reset()` の `expect_no_body = false` を削除する (`request_method = None` は
  現状どおり残す)
- 注: `decode()` の Keep-Alive リセットでは `start_line` はクリアされないが、
  これは `decode_headers()` 内で `self.start_line.take()` により既に消費済みの
  ため不要。`decoded_head` も同様に `.take()` 済み。問題ない。

### examples/http11_reverse_proxy/src/main.rs

- `stream_response_on_connection()` 内の HEAD 専用分岐
  (`if method.eq_ignore_ascii_case("HEAD") { decoder.set_expect_no_body(true); }`)
  を撤去し、代わりにメソッド無条件で `decoder.set_request_method(method)` を呼ぶ
- リクエストデコーダーから取得した `method` を変換せずにそのまま
  `set_request_method` に渡す。`determine_body_kind` 内の `==` 比較は
  case-sensitive なので、小文字化等の変換をすると判定が壊れる。
  RFC 9110 Section 9.1 にも

  > The method token is case-sensitive because it might be used as a gateway
  > to object-based systems with case-sensitive method names.

  と規定されており、case 変換せずそのまま比較するのが RFC 準拠の挙動
- 同じ関数内の `let is_head = method.eq_ignore_ascii_case("HEAD");` は
  **撤去しない**。この変数はデコーダー設定ではなく、プロキシが HEAD レスポンスの
  Content-Length ヘッダーを転送するか否かの判定に使われており、`determine_body_kind`
  では代替できない。以下の分岐は維持する:

  ```rust
  BodyKind::None if is_head => resp_head.content_length(),
  ```

### tests/test_decoder.rs

- `set_expect_no_body(true)` を `set_request_method("HEAD")` に置換する
  (該当箇所 2 つ: `test_head_ignores_invalid_te` と `test_head_ignores_invalid_cl`)
- `test_connect_2xx_tunnel_mode`: status ループから `204` を削除する。
  RFC 9112 Section 6.3 item 1 の優先により CONNECT + 204 は `BodyKind::None` になる
  ため。代わりに CONNECT + 204 → `BodyKind::None` を確認するテストを追加する
  (`test_connect_204_no_body` 等)

### pbt/tests/prop_decoder/response.rs

- `set_expect_no_body(true)` を `set_request_method("HEAD")` に置換する
  (該当箇所 3 つ:
  `prop_head_response_with_content_length`、
  `prop_head_response_with_transfer_encoding`、
  `prop_response_decoder_reset_expect_no_body`)
- `prop_response_decoder_reset_expect_no_body` を
  `prop_response_decoder_reset_request_method` にリネームし、
  「reset 後は `request_method` がクリアされる」を検証する形に変える。
  内部のコメント `reset 後は expect_no_body がクリアされる` も
  `reset 後は request_method がクリアされる` に書き換える
- `prop_connect_all_2xx_tunnel`: 生成範囲に `prop_assume!(status != 204)` を追加する。
  RFC 9112 Section 6.3 item 1 の優先により CONNECT + 204 は `BodyKind::Tunnel` ではなく
  `BodyKind::None` になるため
- `prop_response_decode_tunnel_error`: 同上。`status in 200u16..300` に 204 が含まれるため
  `prop_assume!(status != 204)` を追加する。CONNECT + 204 は `BodyKind::None` のため
  `decode()` がエラーにならず、`prop_assert!(result.is_err())` が失敗する
- 加えて、以下の Keep-Alive 動作を検証する PBT を追加する:
  - `prop_head_request_method_cleared_on_decode_headers_complete`:
    `set_request_method("HEAD")` + `HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n`
    を `decode_headers()` で処理後、続けて
    `HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello` を `decode_headers()`
    で処理し、後者の `body_kind` が `BodyKind::ContentLength(5)` であることを
    検証する (Complete 遷移時の `request_method` リセットを検証)
  - `prop_head_request_method_cleared_on_decode_complete`:
    `set_request_method("HEAD")` + 空ボディレスポンスを `decode()` で処理後、
    続けて `HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello` を `decode()` で
    処理し、後者のレスポンスのボディが `Some(b"hello")` であることを検証する
    (`decode()` 完了時の `request_method` リセットを検証)

### fuzz/fuzz_targets/fuzz_decoder_response.rs

- `decoder.set_expect_no_body(true);` を `decoder.set_request_method("HEAD");`
  に置換する
- ファイル先頭のコメント `set_expect_no_body(true) でのデコードパスを検証する`
  も `set_request_method("HEAD") でのデコードパスを検証する` に更新する

### README.md

- `decoder.set_expect_no_body(true); // HEAD レスポンスではボディなし` を
  `decoder.set_request_method("HEAD"); // HEAD レスポンスではボディなし` に
  置換する

### skills/shiguredo-http11/SKILL.md

- API リファレンス表から `set_expect_no_body()` を削除し、表内に既に存在する
  `set_request_method()` の説明を「HEAD/CONNECT 判定用のリクエストメソッドを設定」に
  更新する
- サンプルコードの `decoder.set_expect_no_body(true);` を
  `decoder.set_request_method("HEAD");` に置換する

### CHANGES.md

`## develop` セクションに以下のエントリを追加する:

```
- [CHANGE] `ResponseDecoder::set_expect_no_body` を撤去し、HEAD レスポンスの指定を `set_request_method("HEAD")` に統一する
  - `expect_no_body` フィールドと `request_method` フィールドの二重化を解消し、`request_method` 一本に集約する
  - `determine_body_kind` の判定順序を RFC 9112 Section 6.3 の優先順位に合わせる (CONNECT + 204 の挙動が Tunnel → None に変わる)
  - @voluntas
- [FIX] `decode_headers()` の Complete 遷移時と `decode()` 完了時に `request_method` をクリアする
  - CONNECT 4xx レスポンス後に後続の 2xx レスポンスが誤って Tunnel 判定される Keep-Alive 状態漏れバグを修正する
  - @voluntas
```

## 検証方針

### HEAD レスポンス = ボディなし の挙動が保たれることの確認

提案後の `determine_body_kind` フローでは、`request_method == "HEAD"` の場合は
RFC 9112 Section 6.3 item 1 の判定で `BodyKind::None` が確定する。Content-Length /
Transfer-Encoding の値に関わらずボディなしと判定されるので、現状の `expect_no_body`
と等価。

既存の HEAD 系テストは `set_expect_no_body(true)` を `set_request_method("HEAD")`
に置換するだけで意味が保たれる:

- `tests/test_decoder.rs::test_head_ignores_invalid_te` (HEAD + 不正な
  Transfer-Encoding でも `BodyKind::None`)
- `tests/test_decoder.rs::test_head_ignores_invalid_cl` (HEAD + 不正な
  Content-Length でも `BodyKind::None`)
- `pbt/tests/prop_decoder/response.rs::prop_response_decoder_reset_request_method`
  (200..=299 の任意 status で HEAD なら `BodyKind::None`、reset 後は通常モード)

これらが提案後コードでも green になることをもって、HEAD レスポンスのボディなし
判定が回帰していないことを確認する。

### CONNECT トンネル判定の挙動が保たれることの確認

CONNECT + 2xx (204 を除く) のレスポンスが `BodyKind::Tunnel` になることは
維持される。具体的には以下のテストがそのまま green になる必要がある:

- `test_connect_non_2xx_normal_body` (非 2xx → Tunnel ではない)
- `test_connect_2xx_ignores_body_headers` (2xx + TE/CL を無視)
- `test_connect_take_remaining`
- `test_connect_tunnel_decode_headers_error`
- `test_connect_tunnel_decode_error`
- `test_response_consume_body_in_tunnel_error`
- `prop_connect_all_2xx_tunnel` (`prop_assume!(status != 204)` 追加後)
- `prop_response_decode_tunnel_error` (`prop_assume!(status != 204)` 追加後)
- `prop_response_take_remaining_tunnel`

### CONNECT + 204 の挙動変更確認 (RFC 準拠への修正)

RFC 9112 Section 6.3 の "in order of precedence" により、item 1 (1xx/204/304 は
ボディなし) が item 2 (CONNECT 2xx はトンネル) より優先される。そのため
CONNECT + 204 は `BodyKind::None` になる。

- `test_connect_2xx_tunnel_mode`: ループから 204 を削除し、別途
  CONNECT + 204 → `BodyKind::None` のテストを追加する
- `prop_connect_all_2xx_tunnel`: `prop_assume!(status != 204)` を追加する
- `prop_response_decode_tunnel_error`: `prop_assume!(status != 204)` を追加する

### 状態漏れバグの修正確認

新規 PBT `prop_head_request_method_cleared_on_decode_headers_complete` と
`prop_head_request_method_cleared_on_decode_complete` が green になることで、
`request_method` が Complete 遷移時および `decode()` 完了時に正しくクリアされる
ことを確認する。

### RFC 準拠性の確認

提案の各判定は以下の RFC 節に準拠している:

- HEAD / 1xx / 204 / 304 → ボディなし: RFC 9112 Section 6.3 item 1
  (CONNECT + 204 も item 1 に吸収され、Tunnel ではなく None になる)
- CONNECT 2xx (204 を除く) → トンネルモード: RFC 9112 Section 6.3 item 2 および
  RFC 9110 Section 9.3.6
- HTTP/1.0 + Transfer-Encoding 拒否: RFC 9112 Section 6.1
  (HEAD/1xx/204/304 は item 1 で先に返るため、このチェックには到達しない:
  RFC 9112 Section 6.3 item 1 の "regardless of the header fields present" に
  準拠)
- TE/CL 解析: RFC 9112 Section 6.3 item 3〜8 (item 7 はリクエスト用のため非該当。
  item 8 が close-delimited フォールバック)
- 205 Reset Content の扱い: RFC 9110 Section 15.3.6 (送信者制約のみ。受信側は
  `status_has_body` で true 扱いとし、TE/CL に従う。現状維持)
- メソッドトークンの case-sensitive 比較: RFC 9110 Section 9.1

## 受け入れ基準

- `make fmt && make clippy && make check && make test` がすべて成功する
- `cargo llvm-cov` で `determine_body_kind` の HEAD 分岐 / CONNECT 分岐 /
  HTTP/1.0+TE 分岐 / 1xx/204/304 分岐のすべてがカバーされている
- 既存の HEAD 系テスト・PBT が `set_request_method("HEAD")` への置換だけで green
  になる (HEAD 挙動の回帰なし)
- `prop_assume!(status != 204)` を追加した `prop_connect_all_2xx_tunnel` を含む
  既存の CONNECT 系テスト・PBT が green になる
- CONNECT + 204 → `BodyKind::None` を確認するテストが追加され green になる
- 新規 PBT で「`Complete` 遷移後・`decode()` 完了後に `request_method` がクリア
  される」ことが検証されている
- fuzz target が `set_request_method("HEAD")` で正常に動作する
