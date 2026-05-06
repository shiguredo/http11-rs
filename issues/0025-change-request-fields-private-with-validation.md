# 0025: Request のフィールドを非公開化しバリデート付き構築に統一する

Created: 2026-05-06
Model: Opus 4.7

## 概要

`Request` 構造体の全フィールド (`method`, `uri`, `version`, `headers`, `body`) を非公開化し、バリデート付きコンストラクタと setter API による構築に統一する。構造体には `#[non_exhaustive]` を付与し、将来のフィールド追加による破壊的変更を防ぐ。

`src/decoder/request.rs` の構造体リテラル構築はフィールド非公開化でコンパイル不能になるため、`pub(crate) fn from_raw_parts(...)` を新設し、デコーダー内のみ検証済みフィールドで直接構築可能にする。

破壊的変更。`Request { ... }` の構造体リテラル構築、および直接フィールド代入 (`request.headers.push(...)` / `request.method = ...` / `request.uri = ...` / `request.body = ...`) は全箇所で禁止される。呼び出し側はすべて新 API に書き換える。

ブランチ名は CLAUDE.md「git ブランチの命名規則」に従い `feature/change-request-fields-private-with-validation` を使用する。

依存関係:
- 本 issue は `0017` (Response フィールド非公開化) の完了後に着手する
- `0017` で確立した「フィールド非公開化 + バリデーション + `from_raw_parts` + `#[non_exhaustive]`」のパターンを Request にそのまま適用する
- `0020` (StatusClass enum) と `0024` (StatusCode 型導入) は Request 側で直接使用しないため依存しない。ただし実装時に `0017` / `0020` / `0024` / `0025` のマージ順序により `git merge` のコンフリクトが発生する可能性があるため、`0017` 〜 `0024` がすべて完了した develop ブランチからブランチを切ることを推奨する
- 本 issue 完了後、別 issue で `Method` 型 + `Request::with_method` を追加予定 (StatusCode 型導入 (`0024`) に相当する Method 型導入)
- 本 issue 完了後、別 issue で Request 側の builder/mutator API 一貫化を追加予定 (`0021` の Request 版)
- 注: 本 issue の `add_header` / `set_header` は `Result<(), EncodeError>` を返すが、後続の `0021` Request 版で `Result<&mut Self, EncodeError>` に変更される予定。これは `0017` → `0021` と同様の二段階破壊的変更であり、API 安定化の過渡期であることを issue 内で明示する

## 実装前提 — 既存実装の確認

本 issue の対応を正しく行うため、以下の既存実装を把握する必要がある:

| 項目 | ファイル | 状態 |
|---|---|---|
| `is_valid_method` | `src/validate.rs:70-72` | **既存** — `!method.is_empty() && method.bytes().all(is_token_char)` |
| `is_valid_request_target` | `src/validate.rs:179-225` | **既存** — 制御文字拒否・RFC 3986 除外文字拒否・パーセントエンコーディング検証・%00 拒否。obs-text (0x80-0xFF) は受信側互換性のため許容。**本 issue ではこの既存実装を流用し、簡易版への置き換えは行わない** |
| `is_valid_protocol_version` | `src/validate.rs:85-125` | **既存** — `token "/" DIGIT+ "." DIGIT+` 形式。RTSP 互換のため DIGIT+ (RFC 9112 §2.3 の `DIGIT "." DIGIT` より広い) と token (RFC 9112 §2.3 の `%s"HTTP"` case-sensitive を強制しない) を許容 |
| `EncodeError::InvalidMethod` | `src/error.rs:71` | **既存** — `{ method: String }`。Display 実装も完備。**新設不要** |
| `EncodeError::InvalidRequestTarget` | `src/error.rs:73` | **既存** — `{ uri: String }`。Display 実装も完備。**新設不要** |
| `validate_request_fields` | `src/encoder.rs:173-210` | **既存** — method / URI / version / headers の全検証を実装済み。`encode_request` (line 649) と `encode_request_headers` (line 962) から呼び出されている。**新設不要。修正対象** |
| `RequestHead` フィールド | `src/decoder/head.rs:110-119` | `pub method` / `pub uri` / `pub version` / `pub headers` — `from_raw_parts` に所有値で渡すための前提 |

本 issue の主な作業は、上記の既存実装を **構築時にも適用する** こと、および **フィールド非公開化に伴う全アクセス箇所の修正** である。新規のバリデーション関数やエラーバリアントの追加は不要。

注: `is_valid_request_target` は「受信側互換性のため obs-text (0x80-0xFF) を許容」するが、これは **RFC 上の根拠がない実装判断** である。request-target の構文定義 (RFC 9112 §3.2) には obs-text は登場せず、`field-vchar` や `reason-phrase` の定義で obs-text が現れるのみである (RFC 9110 §5.5, RFC 9112 §4)。本 issue では **送信側 (構築時) に加え decoder 側でも obs-text を拒否** する (構築された Request は送信されることを前提とするため)。これにより「受信側互換性のため obs-text 許容」は実質的に無効化される。両者の一貫性を取るための obs-text 許容撤去 (validate.rs の `is_valid_request_target` 修正) はスコープ外とし、別 issue で対応する。構築時と decoder の obs-text 拒否チェックはそれまでの暫定措置である。

## 根拠

### 問題 1: HTTP Request Smuggling (CWE-444) の温床

現状すべてのフィールドが `pub` のため、構築後に以下のような不正値の代入が可能:

- `request.method = "GET\r\nX: y".to_string()` (CRLF 注入)
- `request.uri = "/api?q=x HTTP/1.1\r\nGET /admin".to_string()` (request smuggling ペイロード)
- `request.headers.push(("Transfer-Encoding".to_string(), "chunked\r\nContent-Length: 0".to_string()))` (TE/CL 競合の偽装)
- `request.headers.push(("Bad Name".to_string(), "x".to_string()))` (token 違反)
- `request.headers.push(("X-Header".to_string(), "x\0y".to_string()))` (NUL 混入 — RFC 9110 Section 5.5)
- `request.version = "garbage".to_string()` (不正バージョン)
- `request.body = Some(vec![b'x'])` (body を勝手に差し替えられる)

CRLF / NUL 注入は **HTTP Request Smuggling (CWE-444)** の温床で、特に reverse proxy 経路では致命的な脆弱性を生む。RFC 9112 Section 3（Request Line）も lenient parsing が security vulnerabilities を引き起こすと警告している (Section 11.2 参照)。さらに RFC 9112 Section 3.2 は不正な request-line の自動修正を明確に禁止している (SHOULD NOT attempt to autocorrect — セキュリティフィルタのバイパスを防ぐため)。RFC 9112 Section 2.2 も「不正な行の拒否または除去が request smuggling 防御に必要」と述べている。

さらに RFC 9110 Section 5.4 でも、ヘッダー制限の文脈で request smuggling 攻撃 (RFC 9112 Section 11.2) への脆弱性増大を警告している。

- Response Splitting (CWE-113): クライアントを騙す
- Request Smuggling (CWE-444): **バックエンドサーバーを騙す**、認証・認可をバイパスできる

本ライブラリは `examples/http11_reverse_proxy` を提供しており、reverse proxy ユースケースを想定している。Request 側の堅牢性は **本ライブラリの中核要件**。

CLAUDE.md「性能より堅牢性を優先」「一切妥協しない」「サンプルは **お手本** なので性能と堅牢性を両立」に正面から関わる。

### 問題 2: バリデーションが encoder 任せに遅延している

`Request::new`, `with_version`, `add_header`, `header` のいずれも構築時バリデーションが皆無で、エラーが `encode_request` 実行時まで遅延される。本来は構築時点で検出可能な不正値が、ボディを書き込もうとする段階まで持ち越される。`Result` を返す構築 API なら、利用側のエラーハンドリングが「構築時」に集約され、検出が早くなる。

これは Response 側 (`0017`) と同じ問題だが、Request 側では smuggling という具体的脅威が伴うため実利が大きい。

### 問題 3: ヘッダーの case-insensitive 重複が野放し

HTTP ヘッダー名は case-insensitive (RFC 9110 Section 5.1) だが、`add_header("Host", ...)` と `add_header("host", ...)` を両方追加できてしまう。Request 側では特に `Host` / `Transfer-Encoding` / `Content-Length` の重複が smuggling の経路となる。

本 issue では保存側の正規化は行わず (**別 issue で対応**)、「検索のみ case-insensitive」の現状を維持しつつ、**入力時の token / 値バリデーションを追加する範囲に留める**。`set_header` の追加で同名ヘッダーの上書きを可能にする。

### 問題 4: フィールド追加が破壊的変更になる

`pub struct Request { pub ... }` で全フィールド公開のため、将来 `trailers` 等を追加すると、構造体リテラル `Request { ... }` を使う全利用者がコンパイル不能になる。全フィールド非公開化により将来のフィールド追加は非破壊的になる。`#[non_exhaustive]` は構造体リテラル構築の破壊防止を型レベルで宣言する。注: フィールド非公開化後は外部クレートが構造体パターンマッチを行うことはそもそも不可能であり (プライベートフィールドを含む構造体はパターン分解できない)、`#[non_exhaustive]` の効能は「将来フィールドが追加されても外部クレートからの構造体リテラル構築が破壊されないこと」に限定される。

### 問題 5: Host ヘッダーが HTTP/1.1 で MUST であることの検証不在

RFC 9112 Section 3.2 は HTTP/1.1 リクエストには Host ヘッダーが必須 (MUST) であり、欠落した場合は 400 Bad Request を返すべき (SHOULD) と規定している (§3.2: "A client MUST send a Host header field in all HTTP/1.1 request messages")。しかし `Request::new` (HTTP/1.1 固定) は Host ヘッダーの存在を全く検証しない。本 issue のスコープは構文レベルのバリデーション (method/URI/version/headers の文字集合) に限定し、Host ヘッダーの存在検証は encoder 時の `validate_host_header` に委ねる (現状維持)。ただし、この RFC MUST 要件への言及は根拠として明記しておく。

## 対応方針

### 0017 のパターンを踏襲

本 issue は Response 側 (`0017`) で確立したパターンを **そのまま Request に適用** する:

| Response (0017) | Request (本 issue) |
|---|---|
| `Response::new(code, reason)?` | `Request::new(method, uri)?` |
| `Response::with_version(v, c, r)?` | `Request::with_version(method, uri, version)?` |
| `Response::with_status(StatusCode::OK)` (infallible) | 本 issue のスコープ外 (将来 `Method` 型追加 issue で対応) |
| `pub(crate) fn Response::from_raw_parts(...)` | `pub(crate) fn Request::from_raw_parts(...)` |
| status_code バリデーション (`is_valid_status_code`) | method バリデーション (`is_valid_method` — **既存**) |
| reason_phrase バリデーション (`is_valid_reason_phrase`) | uri バリデーション (`is_valid_request_target` — **既存**) |
| `body_bytes()` getter | `body_bytes()` getter (0017 の命名に揃える) |
| `is_body_omitted()` getter | 該当なし (Request には omit_body がない) |
| 共通: version / header / non_exhaustive | 共通: version / header / non_exhaustive |

### 影響範囲一覧

| ファイル | 種別 | 内容 |
|---|---|---|
| `src/request.rs` | 主要変更 | フィールド非公開化、コンストラクタ `Result` 化、`pub(crate) from_raw_parts` 新設 (`debug_assert!` 付き)、アクセサ・setter 追加 |
| `src/validate.rs` | **変更不要** | `is_valid_method`, `is_valid_request_target`, `is_valid_header_name`, `is_valid_field_value`, `is_valid_protocol_version` は全て既存。**新規追加不要**。ただし構築時バリデーション経由で未カバー分岐が発生する可能性があるため、カバレッジ取得対象に含める |
| `src/encoder.rs` | 修正 | Request 関連の全フィールド直接アクセスをアクセサ経由に書き換え (後述の全関数リスト参照)。`validate_request_fields` の二重バリデーション維持。`impl Request` の `encode()` / `try_encode()` の doc comment と expect メッセージを構築時バリデーション導入後の panic 条件に合わせて更新 (0017 の `Response::encode()` doc 更新と同水準)。内部 `#[cfg(test)] mod capacity_tests` の `Request::new()` / `.header()` に `.unwrap()` 追加 |
| `src/decoder/request.rs` | 修正 | 構造体リテラル (line 653-659) → `from_raw_parts` に書き換え。加えて decoder 側の URI 検証に obs-text 拒否を追加 (`from_raw_parts` の不変条件を decoder 側で満たすため)。具体的には `is_valid_request_target` 通過後 (line 334 の `parts[1]` 検証直後) に `parts[1].bytes().any(\|b\| b >= 0x80)` チェックを追加し、不正な場合は `Error::InvalidData("invalid request-target: non-ASCII characters".to_string())` を返す。注: この obs-text 拒否追加は `is_valid_request_target` の「受信側互換性のため obs-text 許容」方針からの逸脱であり、別 issue で validate.rs 側の obs-text 許容撤去が完了するまでの暫定措置である |
| `src/decoder/mod.rs` | 修正 | doctest の `request.method` → `request.method()` に書き換え (line 84) |
| `src/error.rs` | **変更不要** | `InvalidMethod`, `InvalidRequestTarget`, `InvalidHeaderName`, `InvalidHeaderValue`, `InvalidVersion` は全て既存。**新規追加不要** |
| `examples/http11_client/src/main.rs` | 修正 | `Request::new(...)` / `add_header(...)` / `body = ...` を新 API に書き換え、関数シグネチャを `Result<Request, EncodeError>` 化 |
| `examples/http11_reverse_proxy/src/main.rs` | 修正 | upstream のヘッダーをループで `add_header` する箇所を `add_header(name, value)?` に変更。`body = None` 代入 → 条件分岐で `body(data)` 呼び出しの有無を制御する方式に変更。line 389-394 の debug ログ内 `upstream_request.body.as_deref()` → `upstream_request.body_bytes()` に書き換え。関数シグネチャを `Result<Request, EncodeError>` 化。`.unwrap()` は使わない |
| `examples/http11_server/src/main.rs` | 修正 | line 280-286 の `Request { ... }` 構造体リテラル構築を `Request::with_version(...)?` + `add_header` ループ + 条件付き `body(data)` に書き換え。加えてファイル全体の全フィールド直接アクセスをアクセサ経由に書き換え: line 291-296 の `info!` マクロ内の `request.method` / `request.uri` / `request.version`、line 501 の `request.method`、line 504-508 / line 570-574 の `&request.headers` loop および `request.body.as_deref()`、line 567 の `request.method` / `request.uri` / `request.version`。`HttpHead` が未 import のため `use shiguredo_http11::HttpHead` を追加する。`examples` は外部 crate のため `from_raw_parts` 使用不可 |
| `examples/http11_server_io_uring/src/main.rs` | 修正 | `decode()` API から取得した Request の全フィールド直接アクセスをアクセサ経由に書き換え: line 651-653 の `info!` マクロ内の `request.method` / `request.uri` / `request.version`、line 825 の `request.method.eq_ignore_ascii_case(...)` → `request.method().eq_ignore_ascii_case(...)`、line 828-832 の `request.headers.iter().find(...)` → `HttpHead::headers(&request).iter().find(...)`、line 836 の `request.uri.as_str()` → `request.uri()`、line 888-890 の `format!` 内の各アクセス、line 893 の `&request.headers` loop → `HttpHead::headers(&request)` loop、line 897-899 の `request.body` アクセス → `if let Some(req_body) = request.body_bytes()` 形式に書き換え (`request.body` は `Option<Vec<u8>>` のため `.is_empty()` / `.len()` / `&request.body` の直接操作は現在もコンパイル不能。http11_server 側と同様のパターンを使用する)。`HttpHead` が未 import のため `use shiguredo_http11::HttpHead` を追加する |
| `tests/test_request.rs` | **新設** | バリデーションエラー再現テスト、smuggling ペイロード拒否テスト |
| `tests/test_encoder.rs` | 修正 | Request 構築の `.unwrap()` 追加、不正値テストの構築時エラー検証への書き換え。影響を受けるテスト関数: `test_encode_request_invalid_version` (L15-30) — `Response::with_version` で不正 version を渡していたパターンを `Request::with_version` でも同様に検証。`test_encode_request_invalid_host_error` (L283) — encoder 側の Host ヘッダー検証テストで、フィールド直接アクセスがあった場合は書き換え。encode 時検証テスト群 (method/URI/version/headers の不正値) は構築時に `Err` を返す形に変更するため、`test_request.rs` に新設するテストで代替し、encoder 側のバリデーション分岐カバレッジは `encoder.rs` の `#[cfg(test)]` で `from_raw_parts` 経由テストにより補填する (0017 の方針と同様) |
| `tests/test_decoder.rs` | 修正 | line 1371-1372 の `request.method` → `request.method()`、`request.body.as_deref()` → `request.body_bytes()` に書き換え |
| `pbt/tests/prop_request.rs` | 修正 | 全テストのフィールド直接アクセス → accessor に書き換え + バリデーション PBT 追加 |
| `pbt/tests/prop_encoder.rs` | 修正 | `Request::new()` / `.header()` に `.unwrap()` 追加 |
| `pbt/tests/prop_decoder/request.rs` | 修正 | フィールド直接アクセスをアクセサ経由に書き換え |
| `pbt/tests/prop_decoder/body.rs` | 修正 | `request.body.as_deref()`, `request.method`, `request.uri` の直接アクセスをアクセサ経由に書き換え |
| `fuzz/fuzz_targets/fuzz_encode_request.rs` | 修正 | フィールド代入 → setter API に書き換え。`Request::with_version` が `Result` 化されるため、fuzz の任意入力を扱う箇所では `let Ok(mut request) = Request::with_version(...) else { return; };` パターンを使用する。`add_header` も `Result` 化されるため `is_err()` 時に `return;` する。`body = None` 復帰は条件分岐で制御 |
| `fuzz/fuzz_targets/fuzz_decoder_roundtrip.rs` | 修正 | `Request::new` / `Request::with_version` が `Result` 化されるため、`let Ok(mut request) = Request::new(...) else { return; };` パターンを使用する。`add_header` も `Response` 側と同様に `is_err()` 時にループを継続 (`continue`) または `return` する。加えて line 102 の `request.body.unwrap_or_default()` → `request.body_bytes().map(\|b\| b.to_vec()).unwrap_or_default()` に書き換え (所有値が必要なため `unwrap_or(&[])` では不足)。`has_body` 変数は Request 側に存在しないため、既存の `!fuzz_req.body.is_empty()` 条件で `body()` 呼び出しを行う |
| `fuzz/fuzz_targets/fuzz_request_response_helpers.rs` | 修正 | `add_header` の戻り値チェック追加、`body = Some(body)` → `body(body)` に書き換え |
| `fuzz/fuzz_targets/fuzz_decoder_chunked.rs` | 修正 | `Request::new("POST", "/")` → `Request::new("POST", "/").unwrap()`、`request.add_header(...)` → `request.add_header(...).unwrap()` に書き換え。fuzz の任意入力がバリデーションを通過しなくなった場合は早期 return（`if request.add_header(...).is_err() { return; }` パターン）|
| `src/lib.rs` | 修正 | doctest の `Request::new()` / `.header()` に `.unwrap()` 追加 |
| `skills/shiguredo-http11/SKILL.md` | 修正 | Request の主要メソッド一覧を新 API に追従させ、サンプルコードに `.unwrap()` を追加 |

### encoder.rs 内の直接フィールドアクセス書き換え対象関数

以下の関数内で `request.method`, `request.uri`, `request.version`, `request.headers`, `request.body` に直接アクセスしている。全箇所をアクセサ経由に書き換える:

| 関数 | 行番号 (付近) | 直接アクセス内容 |
|---|---|---|
| `should_auto_emit_content_length_for_request` | 79 | `request.body.is_some()`, `request.has_header(...)` |
| `estimate_request_capacity` | 108-130 | `request.method.len()`, `request.uri.len()`, `request.version.len()`, `&request.headers` loop, `request.body.as_deref()` |
| `validate_request_fields` | 173-210 | `&request.method`, `request.method.clone()`, `&request.uri`, `request.uri.clone()`, `request.uri.bytes()`, `&request.version`, `request.version.clone()`, `&request.headers` |
| `validate_host_header` | 418-514 | `request.version`, `request.headers` loop, `request.uri`, `request.uri.contains(...)`, `request.uri.rfind(...)`, `request.method` |
| `encode_request` | 645-724 | `&request.uri`, `request.uri.as_bytes()`, `request.method.as_bytes()`, `request.version.as_bytes()`, `&request.headers` loop, `validate_content_length_headers(&request.headers)?` (line 673 — 関数引数としての直接アクセス)、`request.body.as_deref()`, `request.has_header(...)` |
| `encode_request_headers` | 962+ | `&request.uri`, `request.method.as_bytes()`, `request.uri.as_bytes()`, `request.version.as_bytes()`, `&request.headers` loop |
| `impl Request` (encode + try_encode) | 850-865 | フィールドアクセスなし (委譲のみ)。doc comment と expect メッセージの更新あり: encode() の doc を「構築時バリデーションで弾かれる構文上の不正値を含まない Request ならば、意味論的な RFC 違反がある場合にパニックする」に更新。expect メッセージは `Response::encode()` と文体を揃え `"invalid request fields or headers"` に変更 |
| `impl Request` (encode_headers + try_encode_headers) | 1085-1100 | フィールドアクセスなし (委譲のみ)。doc comment と expect メッセージの更新あり: `encode()` と同様に doc を意味論的違反のパニック条件に更新し、expect メッセージを `"invalid request fields or headers"` に変更 |
| `#[cfg(test)] mod capacity_tests` | 1332-1387 | `Request::new(...).header("Host", ...)` → `Request::new(...).unwrap().header("Host", ...).unwrap()` の追加 (new + header それぞれに `.unwrap()` が必要。L1334 の `let mut req = Request::new("GET", "/"); req = req.header("Host", "x").header("Connection", "close")` のような再代入パターンでは各 `.unwrap()` を付加する)。L1366-1373 の for ループ内 `req = req.header(name, value)` は `req = req.header(name, value).unwrap()` に変更 |

`HttpHead` トレイト経由でアクセス可能なメソッド (`has_header`, `get_header`, `get_headers`, `connection`, `content_length`, `is_chunked`, `is_keep_alive`) は既に `HttpHead` のデフォルト実装経由で動作しており、ヘッダー名 (`name`) 経由のアクセスはすべてトレイトの `headers()` メソッドを経由するため、直接フィールドアクセスは `request.headers` の走査ではなく `self.headers()` 経由で行うよう修正する。

### src/request.rs

#### フィールド非公開化と `#[non_exhaustive]`

```rust
/// HTTP リクエスト
///
/// `body` フィールドは「ボディなし」と「明示的な空ボディ」を区別する。
/// - `None`: ボディを送る意図がない (`Content-Length` を自動付与しない)
/// - `Some(vec![])`: 明示的に空ボディ (`Content-Length: 0` を自動付与)
/// - `Some(data)`: 通常のボディ (`Content-Length: N` を自動付与)
///
/// 全フィールドは非公開で、構築時バリデーション付きの `new` / `with_version` /
/// `header` / `add_header` / `set_header` 経由でのみ操作できる。
/// `#[non_exhaustive]` の効能については「## 根拠」の問題 4 を参照。
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Request {
    method: String,
    uri: String,
    version: String,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
}
```

#### デコーダー用 `pub(crate)` コンストラクタ (新設)

```rust
/// 検証済みの生フィールドから Request を構築 (デコーダー内部用)
///
/// デコーダー側で既にバリデーション済みのフィールドを直接受け取る。
/// コンストラクタのバリデーションはスキップする。
/// 外部クレートからはアクセス不可 (`pub(crate)`)。
///
/// # 不変条件 (呼び出し側の責務)
///
/// 呼び出し側 (decoder) は以下の不変条件をすべて満たすフィールドのみを渡すこと:
/// - `method`: `is_valid_method` を通過済み (RFC 9110 Section 9.1: method = token)
/// - `uri`: `is_valid_request_target` を通過済み。加えて encoder 側の
///   obs-text 拒否 (0x80-0xFF 非含有) を満たすこと
/// - `version`: `is_valid_protocol_version` を通過済み
/// - `headers`: 各エントリが `is_valid_header_name` / `is_valid_field_value` を通過済み
///
/// 引数は所有値 (`String` / `Vec`) を受け取る。decoder 側 (`RequestHead`) が
/// 所有値を保持しているため、move による zero-copy 構築が可能 (Rust API
/// ガイドライン C-OWNED-PARAMETERS に沿う)。
///
/// 注: 命名は標準ライブラリの unsafe 慣習 (`Vec::from_raw_parts` 等) と表面的に
/// 衝突するが、本関数は unsafe ではない。`pub(crate)` のため外部公開 API には
/// 影響しない。crate 内で命名衝突が問題になった場合は、別 issue で
/// `from_decoded_parts` 等への改名を検討する。
pub(crate) fn from_raw_parts(
    method: String,
    uri: String,
    version: String,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
) -> Self {
    // debug ビルドのみで契約を検査する。release では検証スキップ (decoder 経路の最適化)。
    // 契約違反は decoder のバグであり、release で発覚した場合は encoder 側の
    // 二重バリデーション (`validate_request_fields`) が最後の防御線となる。
    debug_assert!(
        crate::validate::is_valid_method(&method),
        "from_raw_parts: invalid method: {method:?}"
    );
    debug_assert!(
        crate::validate::is_valid_request_target(&uri),
        "from_raw_parts: invalid request-target: {uri:?}"
    );
    // 送信側では obs-text を拒否する (validate.rs の is_valid_request_target は
    // 受信側互換性のため obs-text を許容しているが、encoder は追加チェックを行う)
    debug_assert!(
        !uri.bytes().any(|b| b >= 0x80),
        "from_raw_parts: request-target contains non-ASCII: {uri:?}"
    );
    debug_assert!(
        crate::validate::is_valid_protocol_version(&version),
        "from_raw_parts: invalid version: {version:?}"
    );
    debug_assert!(
        headers.iter().all(|(n, v)| {
            crate::validate::is_valid_header_name(n)
                && crate::validate::is_valid_field_value(v)
        }),
        "from_raw_parts: invalid header(s)"
    );
    Self { method, uri, version, headers, body }
}
```

`src/decoder/request.rs:653-659` の構造体リテラルはこのコンストラクタに書き換える:

```rust
// 変更前
Ok(Some(Request {
    method: head.method,
    uri: head.uri,
    version: head.version,
    headers: head.headers,
    body,
}))

// 変更後
Ok(Some(Request::from_raw_parts(
    head.method, head.uri, head.version, head.headers, body,
)))
```

#### コンストラクタの `Result` 化

```rust
impl Request {
    /// 新しいリクエストを作成 (HTTP/1.1)
    ///
    /// バリデーション順序: method → uri。
    /// 失敗時は最初に検出されたエラーを返す。
    ///
    /// `method` は RFC 9110 Section 9.1 の `method = token` (RFC 9110 Section 5.6.2) を要求する。
    /// 検証には既存の `is_valid_method` (validate.rs:70) を流用する。
    /// `uri` は request-target として、CRLF（RFC 9112 §3.2: whitespace 禁止）および
    /// NUL（RFC 9110 §5.5: CR/LF/NUL are invalid and dangerous）を含まないことを要求する (構文レベルのバリデーション)。
    /// request-target 形式 (origin/absolute/authority/asterisk) の判定は
    /// encode 時の `validate_request_target_form` に委ねる。
    /// 検証には既存の `is_valid_request_target` (validate.rs:179) を流用する。
/// 加えて送信側のポリシーとして obs-text (0x80-0xFF) の非含有も確認する
/// (既存の encoder.rs 側のチェックと同等の水準)。
/// obs-text 拒否も `InvalidRequestTarget { uri }` を返す (新規エラーバリアント不要)。
pub fn new(method: &str, uri: &str) -> Result<Self, EncodeError>

    /// カスタムバージョンでリクエストを作成
    ///
    /// バリデーション順序: method → uri → version。
    /// version は `is_valid_protocol_version` (token "/" DIGIT+ "." DIGIT+) で検証する。
    /// RTSP バージョン (RTSP/1.0 等) も受理する。
    /// 注: RFC 9112 §2.3 の `HTTP-name = %s"HTTP"` (case-sensitive) および
    /// `DIGIT "." DIGIT` (各 1 桁) は強制しない。
    /// HTTP として送信する場合は呼び出し側が `"HTTP/1.1"` を渡す責務がある。
    pub fn with_version(method: &str, uri: &str, version: &str) -> Result<Self, EncodeError>
}
```

バリデーション呼び出し:

| コンストラクタ | 検証内容 | 使用関数 |
|---|---|---|
| `new` | method: token | `is_valid_method` (validate.rs:70 — **既存**) |
| `new` | uri: CRLF/NUL/制御文字/RFC3986 除外文字 non-existence + obs-text 非含有 | `is_valid_request_target` (validate.rs:179 — **既存**) + 追加で `uri.bytes().any(\|b\| b >= 0x80)` を拒否。obs-text 拒否時は `InvalidRequestTarget { uri }` を返す (新規エラーバリアント追加不要、method と同様に最初に失敗したバリデーションのエラーを返す) |
| `with_version` | method: 同上 | 同上 |
| `with_version` | uri: 同上 | 同上 |
| `with_version` | version: token `/` DIGIT+ `.` DIGIT+ | `is_valid_protocol_version` (validate.rs:85 — **既存**) |

バリデーション順序:
- `new`: first `is_valid_method` → then `is_valid_request_target` → then `uri.bytes().any(|b| b >= 0x80)` チェック。最初に失敗した検証のエラーを返す
- `with_version`: first `is_valid_method` → `is_valid_request_target` → obs-text チェック → then `is_valid_protocol_version`

注: 構築時には `is_valid_protocol_version` (token "/" DIGIT+ "." DIGIT+ 形式) を使用する。これは `is_valid_version_for_encode` (encoder.rs:18 の内部関数、`!version.is_empty() && version.bytes().all(\|b\| matches!(b, 0x21..=0x7E))` で VCHAR のみを検証) よりも**厳しい**検証であり (tchar ⊂ VCHAR のため、`is_valid_protocol_version` を通過する値は必ず `is_valid_version_for_encode` も通過する)、二重チェックの実益は `from_raw_parts` の release ビルドスキップ後の防御線として機能する。

### URI バリデーションのスコープ

本 issue では URI のバリデーションに **既存の `is_valid_request_target` (validate.rs:179-225)** をそのまま流用する。既存実装は以下を検証済み:

- 制御文字 (0x00-0x20, 0x7F) の拒否 → CRLF 注入の防御 (RFC 9112 §3.2: whitespace 禁止)、NUL 注入の防御 (RFC 9110 §5.5: CR/LF/NUL are invalid and dangerous)
- RFC 3986 除外文字 (`"#<>\\^`{|}`) の拒否 → `#` はフラグメント区切りのため RFC 9112 §3.2 で request-target に含められない
- 不正なパーセントエンコーディング (不完全・非 hex 文字) の拒否
- `%00` (パーセントエンコーディングされた NUL) の拒否
- obs-text (0x80-0xFF) は受信側互換性のため許容 (送信側では encoder 側で別途拒否)。注: この obs-text 許容は RFC 上の根拠がない実装判断である。RFC 9110 §5.5 および RFC 9112 §4 で obs-text が現れるのは `field-vchar` および `reason-phrase` のみで、request-target の構文 (RFC 9112 §3.2) には obs-text は含まれない。本 issue では構築時と decoder の両方で obs-text 拒否チェックを追加するため、この挙動は実質的に無効化される。validate.rs 側の obs-text 許容撤去は別 issue で対応する

構築時には本関数の全チェックに加え、**obs-text 非含有の追加チェック**を行う (encoder.rs の既存の obs-text 拒否チェックと同等の水準を構築時にも適用)。

request-target 形式 (origin/absolute/authority/asterisk) の判定は encode 時の `validate_request_target_form` に委ねる。構築時には `is_valid_request_target` による構文レベル (CRLF/NUL 禁止) で十分であり、形式判定までは行わない。

### ヘッダー追加 / 上書き / 取得 API

`0017` (Response) と完全に並行構造:

```rust
pub fn header(mut self, name: &str, value: &str) -> Result<Self, EncodeError>  // builder
pub fn add_header(&mut self, name: &str, value: &str) -> Result<(), EncodeError>  // mutator
pub fn set_header(&mut self, name: &str, value: &str) -> Result<(), EncodeError>  // 同名上書き
pub fn body(mut self, body: Vec<u8>) -> Self  // builder (infallible)
```

バリデーションは既存の `is_valid_header_name` / `is_valid_field_value` を使用する (`0017` と同一)。

`set_header` の実装は 0017 の Response 側 (`src/response.rs:279-287`) と同じアトミック性保証パターンを使用する:
1. `is_valid_header_name(name) && is_valid_field_value(value)` を **先に** 検証し、失敗時は早期 `Err` で `self` を変更しない
2. バリデーション通過後に `self.headers.retain(|(n, _)| !n.eq_ignore_ascii_case(name))` で同名全削除
3. `self.headers.push((name.to_string(), value.to_string()))` で末尾追加
4. `Ok(())` を返す

#### ヘッダー重複に関する方針

- `add_header` / `header` は同名複数値を許す (Set-Cookie 等の用途で必須)
- `set_header` は同名全削除後に追加 (上書き)。値を空にした場合の削除機能は本 issue では提供しない (RFC 9110 では空値ヘッダーとヘッダー非存在は意味が異なるため)
- 保存は raw (case-preserving)、検索は case-insensitive (現状維持)
- 正規化 (lowercase 強制等) は別 issue で対応

注: `set_header` の「同名全削除→末尾追加」実装により、Host ヘッダーを `set_header` 経由で上書きした場合、Host の位置が末尾に移動する。RFC 9110 §5.3 は「Host on requests を先頭に送るのが good practice」としているが (§5.3: "it is good practice to send header fields that contain additional control data first, such as Host on requests")、MUST 要件ではない。Host の先頭維持は本 issue のスコープ外とし、後続 issue で対応する。

### 読み取り専用アクセサ

```rust
pub fn method(&self) -> &str
pub fn uri(&self) -> &str
pub fn version(&self) -> &str
pub fn body_bytes(&self) -> Option<&[u8]>
```

`version()` は Response 側 (src/response.rs:291) と同様に `Request` の固有メソッドとして提供する。`HttpHead::version(&self)` の実装はそのまま残す（トレイト実装の義務）。

`body_bytes()` は `0017` 完了後の Response 側 (`src/response.rs:309`) と命名を揃える。0017 の実装で builder `body(self, Vec<u8>) -> Self` と getter `body(&self) -> Option<&[u8]>` の同名併存が Rust の inherent impl 制約で不可能であることが判明し、getter は `body_bytes()` に改名された (0017 SOLUTION L745-746)。本 issue もこの命名を踏襲する。

以下は既存の `HttpHead` トレイト実装経由で提供済みのため追加不要:
- `headers()` → `HttpHead::headers` → `&[(String, String)]`
- `get_header()` → `HttpHead::get_header` → `Option<&str>`
- `get_headers()` → `HttpHead::get_headers` → `Vec<&str>`
- `has_header()` → `HttpHead::has_header` → `bool`
- `connection()` → `HttpHead::connection` → `Option<&str>`
- `content_length()` → `HttpHead::content_length` → `Option<u64>`

注: 現在の `src/request.rs:79-118` の `get_header`, `get_headers`, `has_header`, `connection`, `is_keep_alive`, `content_length`, `is_chunked` の重複メソッド群は、0017 完了後の Response 側 (`src/response.rs:319-368`) と同様に **そのまま残す** (HttpHead トレイトへの委譲メソッドとして存続させる)。

### encoder 側の `validate_request_fields`

`src/encoder.rs:173-210` の既存 `validate_request_fields` は、本 issue で追加する構築時バリデーションと合わせて**二重チェックを維持**する。`0017` の方針に従い:

- 構築時バリデーション (本 issue で追加) と encode 時バリデーション (`validate_request_fields`) の二重チェックを維持
- `validate_request_fields` は既存の検証ロジックをそのまま維持する (method → URI → obs-text → request-target form → version → headers)

`validate_request_fields` の各検証分岐は、構築時バリデーションを通過した Request でも必ず通過するよう設計されている。これにより `from_raw_parts` 経由で構築された (release ビルドでは検証をスキップした) Request に対する最終防御線となる。

### 書き換え対応一覧

| 旧 API | 新 API |
|---|---|
| `Request::new("GET", "/")` | `Request::new("GET", "/").unwrap()` (テストのみ) / `Request::new("GET", "/")?` (examples) |
| `Request::with_version(m, u, v)` | `Request::with_version(m, u, v).unwrap()` / `?` |
| `Request { method, uri, ... }` | `Request::new(...).unwrap()` + setter (外部) / `from_raw_parts` (crate 内) |
| `request.headers.push((n, v))` | `request.add_header(n, v)?` |
| `request.add_header(n, v)` | `request.add_header(n, v)?` |
| `.header(n, v)` | `.header(n, v)?` |
| `request.body = Some(data)` | `request = request.body(data)` |
| `request.body = None` | `body = None` への**再代入**は別 issue (Request 版 0021) の `without_body` で対応。本 issue では body の初期値を `None` のままにするために `body(data)` 呼び出しをスキップする条件分岐で対処する (後述の fuzz / examples 改修パターン参照)。body を `None` から `Some(...)` にする `body(data)` ビルダーは本 issue で使用可能 |
| `request.method` (参照) | `request.method()` |
| `request.uri` (参照) | `request.uri()` |
| `request.version` (参照) | `request.version()` |
| `request.headers.len()` (参照) | `HttpHead::headers(&request).len()` |
| `request.body.as_deref()` | `request.body_bytes()` |
| `request.body.is_some()` | `request.body_bytes().is_some()` |
| `request.body.clone().unwrap_or_default()` | `request.body_bytes().map(\|b\| b.to_vec()).unwrap_or_default()` |
| `request.body.unwrap_or_default()` | `request.body_bytes().unwrap_or(&[])` (参照) / `request.body_bytes().map(\|b\| b.to_vec()).unwrap_or_default()` (所有値) |
| `request.headers[0].0` (インデックスアクセス) | `HttpHead::headers(&request)[0].0` |
| `request.headers.iter().find(...)` | `HttpHead::headers(&request).iter().find(...)` |
| `&request.headers` loop | `HttpHead::headers(&request)` loop (for 文は `for (n, v) in HttpHead::headers(&request)` に) |

`Request { ... }` 構造体リテラルの直接構築は全箇所で禁止される。crate 内は `from_raw_parts` を使用する。

### body = None 復帰の対処 (本 issue スコープ内)

`body = None` への復帰は別 issue (0021 の Request 版) で `without_body` 等として対応予定。本 issue のスコープ内では以下の方法で対処する:

#### fuzz ターゲットの改修パターン

```rust
// 変更前
request.body = if body_present { Some(body) } else { None };

// 変更後: 条件分岐で body(data) 呼び出しの有無を制御
let request = if body_present {
    request.body(body)
} else {
    request  // body = None のまま
};
```

- `fuzz_encode_request.rs`: 上記パターンで改修
- `fuzz_decoder_roundtrip.rs`: `has_body && !body.is_empty()` 条件に合わせて条件付きで `body()` を呼び出す
- `fuzz_request_response_helpers.rs`: `request.body = Some(body)` → `request = request.body(body)` に書き換え

#### examples の改修パターン

`examples/http11_reverse_proxy/src/main.rs`:
```rust
// 変更前
upstream_request.body = if matches!(req_body_kind, BodyKind::None) {
    None
} else {
    Some(request_body)
};

// 変更後
let upstream_request = if matches!(req_body_kind, BodyKind::None) {
    upstream_request
} else {
    upstream_request.body(request_body)
};
```

`examples/http11_server/src/main.rs` (line 280-286):
```rust
// 変更前
let request = Request {
    method: head.method,
    uri: head.uri,
    version: head.version,
    headers: head.headers,
    body: state.body.take(),
};

// 変更後 (examples は外部 crate のため from_raw_parts 使用不可)
let mut request = Request::with_version(
    &head.method, &head.uri, &head.version,
)?;
for (name, value) in &head.headers {
    request.add_header(name, value)?;
}
let request = if let Some(body) = state.body.take() {
    request.body(body)
} else {
    request
};
```

### examples の改修方針

CLAUDE.md「サンプルは **お手本**」原則に従い、examples は **`.unwrap()` を使わず `?` 伝播する** 設計に書き換える。特に reverse proxy は smuggling 防御のショーケースとして、動的入力に対して必ずバリデーションを通すサンプルにする。

- `build_request` 等の戻り値を `Result<Request, EncodeError>` に変更し、`?` で伝播
- 静的リテラルのヘッダー (`"Host"`, `"User-Agent"` 等) は `.expect("static header is valid")` を許容するが、メッセージで「リテラルが妥当だから expect」を明示
- 動的入力 (upstream のヘッダー値、URI パラメータ等) には必ず `?` を使う

## CHANGES.md

`## develop` セクションに以下を追加する (AGENTS.md に従い UPDATE → ADD → CHANGE → FIX の順):

```
- [CHANGE] `Request` の全フィールドを非公開化し、構築時バリデーションを追加する
  - 構築は `Request::new` / `Request::with_version` が `Result<Self, EncodeError>` を返す形に変更する
  - `add_header` / `header` でヘッダー名 (RFC 9110 Section 5.1 token) と値 (RFC 9110 Section 5.5 field-value, CR/LF/NUL 不可) をバリデートし `Result` を返す
  - `set_header` を新設し、同名ヘッダー (case-insensitive) の上書きを可能にする
  - method を RFC 9110 Section 9.1 token として、URI を既存の `is_valid_request_target` (制御文字/RFC 3986 除外文字/%00 拒否 + obs-text 非含有) でバリデートする
  - HTTP Request Smuggling (CWE-444) 防御を強化する
  - `pub(crate) fn from_raw_parts` を新設し、デコーダー内部からの検証済み構築を可能にする (debug_assert! 付き)
  - `method()` / `uri()` / `version()` / `body_bytes()` の読み取り専用アクセサを追加する
  - 構造体に `#[non_exhaustive]` を付与する
  - `encoder.rs` 内部の全フィールド直接アクセスをアクセサ経由に書き換える
  - decoder が obs-text (0x80-0xFF) を含む request-target を拒否するよう変更する (送信側と一貫したポリシーを decoder にも適用)
  - @voluntas
```

## 検証方針

### 不変条件が構築時点で守られることの確認

新規単体テスト (`tests/test_request.rs`) で以下を検証する:

- 不正な method (空文字列、CRLF を含む、SP を含む、token 違反文字を含む) で `Err(InvalidMethod)` が返る
- 不正な URI (空文字列、CRLF を含む、NUL を含む、SP を含む、制御文字を含む、RFC 3986 除外文字を含む) で `Err(InvalidRequestTarget)` が返る
- スペースを含むヘッダー名で `Err(InvalidHeaderName)` が返る
- CRLF を含むヘッダー値で `Err(InvalidHeaderValue)` が返る
- NUL を含むヘッダー値で `Err(InvalidHeaderValue)` が返る
- 空ヘッダー値が合法であること (RFC 9110 Section 5.5: `field-value = *field-content`) — `is_valid_field_value` は空文字列で `true` を返す (空イテレータで `.all()` が常に `true`)
- 先頭/末尾に SP/HTAB を含むヘッダー値は合法であること (trim は行わない。RFC 9110 §5.5 は「A field parsing implementation MUST exclude such whitespace prior to evaluating the field value」と MUST で先行/末尾空白の除外を要求しているが、本 issue の目的は smuggling 防御であり、先行/末尾空白の trim を本 issue に含めるとスコープが拡大しすぎる。trim は後続 issue で対応する。本テストは RFC 逸脱の暫定テストであり、後続 issue で修正される動作であることを注記すること)
- token/DIGIT.DIGIT 形式に違反する version で `Err(InvalidVersion)` が返る
- `set_header` が case-insensitive に上書きできる
- `set_header` のバリデーション失敗時に既存ヘッダーが消えない (アトミック性)

### Request smuggling 防御の確認

具体的な smuggling ペイロードを構築時に拒否することを検証する:

```rust
// CRLF 注入による TE/CL 競合の偽装を構築時に拒否する
assert!(matches!(
    request.add_header("Transfer-Encoding", "chunked\r\nContent-Length: 0").unwrap_err(),
    EncodeError::InvalidHeaderValue { .. }
));

// method への CRLF 注入を構築時に拒否する
assert!(matches!(
    Request::new("GET\r\nX: y", "/").unwrap_err(),
    EncodeError::InvalidMethod { .. }
));

// URI への SP 注入 (smuggling の典型ペイロード) を構築時に拒否する
assert!(matches!(
    Request::new("GET", "/api?q=x HTTP/1.1\r\nGET /admin").unwrap_err(),
    EncodeError::InvalidRequestTarget { .. }
));

// URI への %00 NUL エンコーディング (smuggling ペイロード) を構築時に拒否する
assert!(matches!(
    Request::new("GET", "/path%00bad").unwrap_err(),
    EncodeError::InvalidRequestTarget { .. }
));
```

### 既存挙動が回帰しないことの確認

- 既存の単体テスト (`tests/test_encoder.rs`, `tests/test_decoder.rs` 等) が新 API に追従して green になる
- PBT (`prop_request.rs`, `prop_encoder.rs`, `prop_decoder/request.rs`, `prop_decoder/body.rs` 等) が新 API に追従して green になる
- fuzz ターゲット (`fuzz_encode_request`, `fuzz_decoder_roundtrip`, `fuzz_request_response_helpers`) が新 API に追従して green になる
- 全 examples (`http11_server`, `http11_server_io_uring`, `http11_reverse_proxy`, `http11_client`) がコンパイルおよび実行可能である

### カバレッジ検証

```bash
cargo llvm-cov clean --workspace
# request.rs の #[cfg(test)] mod tests を実行
cargo llvm-cov --no-report -p shiguredo_http11 --lib -- request
# validate.rs の検証関数を request テスト経由で計測
cargo llvm-cov --no-report -p shiguredo_http11 --lib -- validate
# encoder.rs — 内部テスト + from_raw_parts 経由の decoder 到達パスを含む
cargo llvm-cov --no-report -p shiguredo_http11 --lib -- encoder
# decoder 経由の from_raw_parts 呼び出しパス
cargo llvm-cov --no-report -p shiguredo_http11 --lib -- decoder
# 新規単体テスト
cargo llvm-cov --no-report -p shiguredo_http11 --test test_request
# 既存テストの回帰確認
cargo llvm-cov --no-report -p shiguredo_http11 --test test_encoder
cargo llvm-cov --no-report -p shiguredo_http11 --test test_decoder
# PBT (request + decoder request/body)
cargo llvm-cov --no-report -p pbt --test prop_request
cargo llvm-cov --no-report -p pbt --test prop_encoder
cargo llvm-cov --no-report -p pbt --test prop_decoder
cargo llvm-cov report
```

`Request::new` / `with_version` / `add_header` / `header` / `set_header` / `from_raw_parts` の全バリデーション分岐 (成功パス・失敗パス) がカバーされていることを確認する。

## 受け入れ基準

- ブランチ名が `feature/change-request-fields-private-with-validation` であること
- `make fmt && make clippy && make check && make test` がすべて成功する
- `src/request.rs` から `pub method` / `pub uri` / `pub version` / `pub headers` / `pub body` が消えている
- `Request` 構造体に `#[non_exhaustive]` が付いている
- `pub(crate) fn from_raw_parts` が Request に存在し、decoder がこれを使用している。かつ `debug_assert!` で method, uri, version, headers の全不変条件が検査されている
- バリデーションエラー再現テストが全種成功する
  - `InvalidMethod` (空文字列, CRLF 含む, SP 含む, token 違反)
  - `InvalidRequestTarget` (空文字列, CRLF 含む, NUL 含む, SP 含む, 制御文字含む, RFC 3986 除外文字含む, **obs-text (0x80-0xFF) 含む**)
  - `InvalidHeaderName` (スペース含む, 空文字列)
  - `InvalidHeaderValue` (CRLF 含む, LF 含む, NUL 含む)
  - `InvalidVersion` (不正形式)
- 空ヘッダー値が合法であることのテストが存在する
- 先頭・末尾に SP を含むヘッダー値が合法であることのテストが存在する (trim は行わない判断の確認)
- `set_header` の上書き動作および case-insensitive 上書きが検証されている
- Request smuggling ペイロード (CRLF 注入による TE/CL 偽装、method/URI への CRLF 注入、URI への %00 注入) を構築時に拒否することのテストが存在する
- `encoder.rs` 内部で `request.method` / `request.uri` / `request.version` / `request.headers` / `request.body` への直接フィールドアクセスが残っていない
- `cargo llvm-cov` でコンストラクタ / `add_header` / `set_header` / `from_raw_parts` のバリデーション分岐がカバーされている
- 全 fuzz ターゲットが新 API に追従しコンパイル可能である
- `set_header` のバリデーション失敗時に既存ヘッダーが消えない (アトミック性) ことが単体テストで検証されている
- 全 examples が `.unwrap()` を使わず `?` 伝播で書かれている (静的リテラルへの `.expect("...")` は理由付きで許容)
- `encoder.rs` 内の `validate_request_fields` の obs-text 拒否分岐が `from_raw_parts` 経由のテストでカバーされている (method / URI / version / headers 分岐は debug ビルドで `from_raw_parts` の `debug_assert!` が先に catch するため到達不能。これらは release ビルドの最終防御線として機能する)
- `skills/shiguredo-http11/SKILL.md` の Request API 一覧およびサンプルコードが新 API に追従している
- `from_raw_parts` の不変条件が `debug_assert!` で検査されており、debug ビルドで契約破りを検出可能であること
- `Request::body_bytes()` が `Response::body_bytes()` (0017 の命名) と一貫した名前であること
