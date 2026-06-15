# Request / Response に into_xxx / into_parts がない

- Priority: Medium
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/refactor-request-response-into-parts
- Polished: 2026-06-16

## 目的

`Request` / `Response` に対し、`RequestHead` / `ResponseHead` / ボディ等の所有権移動を可能にする `into_parts` 系メソッドを追加し、`RequestHead` / `ResponseHead` との API 一貫性を向上させる。

## 優先度根拠

Medium とする。`RequestHead` / `ResponseHead` は `into_xxx` 系メソッドが多数ある一方で、`Request` / `Response` はヘッドを取り出す際に各フィールドをクローンする必要がある。プロキシ等の「ヘッドを改変しつつボディはそのまま流す」処理で不要なコピーが発生している。

## 現状

- `src/request.rs:327` : `Request::body_bytes(&self) -> Option<&[u8]>` のみ。`RequestHead` を所有権移動で取り出すメソッドは存在しない。
- `src/response.rs:409-421` : `Response::body_bytes(&self) -> Option<&[u8]>` と `Response::is_body_omitted(&self) -> bool` のみ。`ResponseHead` を所有権移動で取り出すメソッドは存在しない。
- `src/decoder/head.rs:272-302` : `RequestHead` に `into_method`, `into_uri`, `into_version`, `into_headers`, `into_parts` 等の所有権移動メソッドがある。
- `src/decoder/head.rs:480-509` : `ResponseHead` に `into_version`, `into_reason_phrase`, `into_headers`, `into_parts` 等の所有権移動メソッドがある。
- `src/decoder/head.rs:314-337` : `RequestHead::from_validated_parts` により、バリデーション済みの所有値から `RequestHead` を zero-copy で構築できる。
- `src/decoder/head.rs:519-546` : `ResponseHead::from_validated_parts` により、バリデーション済みの所有値から `ResponseHead` を zero-copy で構築できる。

## 設計方針

### API シグネチャの一貫性方針

closed/0106 で導入された `RequestHead::into_parts(self) -> (Method, String, String, Vec<(HeaderName, String)>)` および `ResponseHead::into_parts(self) -> (String, u16, String, Vec<(HeaderName, String)>)` は **head の構成要素をフラットに返す** シグネチャ。本 issue で追加する `Request::into_parts` / `Response::into_parts` は AGENTS.md「API の一貫性」原則に従い、**同様にフラットな構成要素 + ボディ** を返す形に統一する。head 全体と body のペアを返すメソッドは別名 (`into_head_and_body`) として併設する。

### struct (`RequestParts` / `ResponseParts`) を採用しない理由

5 タプル (Request) / 6 タプル (Response) は要素数が多く、特に `Response::into_parts` 末尾の `bool` (omit_body) は呼び出し側で意味が見えにくい。これを `pub struct RequestParts { method, uri, version, headers, body }` のような構造体で返す案も検討したが、以下の理由で本 issue では採用しない:

- 採用すると `RequestHead::into_parts` (closed/0106) / `ResponseHead::into_parts` (closed/0106) も `HeadParts` 等の struct に揃える破壊的変更が必要となり、本 issue のスコープを超える
- AGENTS.md「Request / Response / RequestHead / ResponseHead 間で API の一貫性」を満たすには、フラットタプル統一かフラット struct 統一のいずれかで全 4 型を揃える必要がある
- フラットタプルは Rust の慣用的なゼロアロケーション戻り型で性能が良く、要素数が多くても rustdoc と型名で文脈は補える
- 将来 struct 化を行う場合は 4 型同時に対応する別 issue として起票する

`Response::into_parts` の末尾 `bool` は rustdoc で「`omit_body` フラグ。pending/0018 完了後に削除予定」と明記する。

### 具体方針

1. `Request::into_parts(self) -> (Method, String, String, Vec<(HeaderName, String)>, Option<Vec<u8>>)` を追加する。
   - 戻り順序は `RequestHead::into_parts` (method, uri, version, headers) に body を末尾に追加した形。
   - 内部では `from_validated_parts` を経由せず、`self` のフィールドを直接ムーブする (再検証不要)。

2. `Response::into_parts(self) -> (String, u16, String, Vec<(HeaderName, String)>, Option<Vec<u8>>, bool)` を追加する。
   - 戻り順序は `ResponseHead::into_parts` (version, status_code, reason_phrase, headers) に body と omit_body を末尾に追加した形。
   - `omit_body` (送信専用フラグ) を末尾に含めることで、proxy で `into_parts` → 改変 → 再構築する典型フローでも HEAD 応答 (`omit_body == true`) が壊れないようにする。批判観点での「情報損失」を防ぐ。

3. `Request::into_head_and_body(self) -> (RequestHead, Option<Vec<u8>>)` を追加する。
   - head 全体を `RequestHead` として保持したまま body と分離したいケース向け (典型: head を別関数に渡してフィルタリング)。
   - 内部実装は `RequestHead::from_validated_parts` を crate 内呼び出し (`pub(crate)`) でゼロコピー構築。

4. `Response::into_head_and_body(self) -> (ResponseHead, Option<Vec<u8>>, bool)` を追加する。
   - body と omit_body を切り離す。
   - 同じく `ResponseHead::from_validated_parts` を crate 内呼び出し。

5. `Request::into_head(self) -> RequestHead` を追加する。
   - ボディは破棄される。

6. `Request::into_body(self) -> Option<Vec<u8>>` を追加する。
   - ヘッドは破棄される。

7. `Response::into_head(self) -> ResponseHead` を追加する。
   - ボディと omit_body は破棄される。omit_body が必要な場合は事前に `is_body_omitted()` で取得するか、`into_head_and_body` / `into_parts` を使う旨を rustdoc に明記する。

8. `Response::into_body(self) -> Option<Vec<u8>>` を追加する。
   - ヘッドと omit_body は破棄される。

### 維持される既存 API

既存の `body_bytes(&self)` およびその他の参照取得 API は **本 issue で変更しない**。完了条件側にもこれは明示しない (変更ゼロのため再掲不要)。

### 将来の `omit_body` 撤去との関係

`Response::omit_body` は pending/0018 で将来撤去予定の送信専用フラグ。撤去後は本 issue で追加する `into_parts` / `into_head_and_body` の戻り型から `bool` (omit_body) を削除する破壊的変更が必要になる。pending/0018 が完了するまでは本 issue の戻り型を維持する。

## 完了条件

- `src/request.rs` に以下が追加されること:
  - `Request::into_parts(self) -> (Method, String, String, Vec<(HeaderName, String)>, Option<Vec<u8>>)` (フラット構成要素 + body)
  - `Request::into_head_and_body(self) -> (RequestHead, Option<Vec<u8>>)` (head 全体と body)
  - `Request::into_head(self) -> RequestHead`
  - `Request::into_body(self) -> Option<Vec<u8>>`
- `src/response.rs` に以下が追加されること:
  - `Response::into_parts(self) -> (String, u16, String, Vec<(HeaderName, String)>, Option<Vec<u8>>, bool)` (フラット構成要素 + body + omit_body)
  - `Response::into_head_and_body(self) -> (ResponseHead, Option<Vec<u8>>, bool)` (head 全体と body と omit_body)
  - `Response::into_head(self) -> ResponseHead` (omit_body と body は破棄、rustdoc で警告)
  - `Response::into_body(self) -> Option<Vec<u8>>`
- 新規メソッドは所有権移動のみを行い、フィールドのクローンを発生させないこと。
- `RequestHead::into_parts` / `ResponseHead::into_parts` (closed/0106 で導入) と戻り型のフラット構成要素 prefix が一致していること (API 一貫性)。
- `tests/test_request.rs` / `tests/test_response.rs` に以下のテストを追加すること:
  - `into_parts` でフラットに構成要素 + body が取得できること (Request/Response 両方)
  - `into_head_and_body` で head 全体 + body が取得できること
  - `into_head` でヘッドのみが取得でき、ボディは破棄されること
  - `into_body` でボディのみが取得でき、ヘッドは破棄されること
  - ボディが `None` / `Some(vec![])` / `Some(data)` の各パターン
  - `Response::into_head` 後、`omit_body` の値が失われること、`is_body_omitted()` を事前取得すれば保持できること (rustdoc 記述の検証)
  - `Response::into_parts` / `into_head_and_body` で omit_body が末尾要素として正しく取得できること
- `CHANGES.md` の `## develop` に以下を追加すること (既存の closed/0106 由来エントリと別エントリ)。
  - `[ADD] Request / Response に into_parts() / into_head_and_body() / into_head() / into_body() を追加し、所有権移動でフィールド・head・body を取り出せるようにする`
- ドキュメントコメントは日本語で記述すること。
- pending/0018 (omit_body 撤去) との関係を rustdoc に注記すること (将来戻り型変更の可能性)。

## 解決方法

設計方針に沿って次の順序で実装する。

1. `src/request.rs` に `into_parts` / `into_head_and_body` / `into_head` / `into_body` を追加する。`into_head_and_body` は `RequestHead::from_validated_parts` を crate 内呼び出しでゼロコピー構築する。
2. `src/response.rs` に同等のメソッド (`into_parts` / `into_head_and_body` / `into_head` / `into_body`) を追加する。`into_parts` / `into_head_and_body` は末尾に `omit_body: bool` を含める。
3. `examples/http11_reverse_proxy` は現状 `RequestDecoder::decode_headers` / `ResponseDecoder::decode_headers` から直接 `RequestHead` / `ResponseHead` を取得して `into_parts()` しているため、本追加メソッドを直接使用する箇所はないが、内部コードで `Request` / `Response` からヘッドを取り出す必要がある箇所があれば適用を検討する。
4. テストを追加する。
5. `CHANGES.md` に `[ADD]` エントリを追加する。

## 単一目的の整理

本 issue は内容的に **新メソッド追加 (add)** が主目的。ファイル名 prefix が `refactor-` だが、AGENTS.md 規約上は新 API 追加なので `add-` が本来正確。本 issue 完了時の Branch 名 (`feature/refactor-request-response-into-parts`) はそのまま維持するが、将来 `create-issue` 経由で類似 issue を起票する際は `add-` prefix を採用する。本 issue ではファイル名 / Branch 名のリネームは行わない (履歴の混乱を避ける)。
