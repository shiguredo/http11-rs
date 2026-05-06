# 0025: Request のフィールドを非公開化しバリデート付き構築に統一する

Created: 2026-05-06
Model: Opus 4.7

## 概要

`Request` 構造体の全フィールド (`method`, `uri`, `version`, `headers`, `body`) を非公開化し、バリデート付きコンストラクタと setter API による構築に統一する。構造体には `#[non_exhaustive]` を付与し、将来のフィールド追加による破壊的変更を防ぐ。

`src/decoder/request.rs` の構造体リテラル構築はフィールド非公開化でコンパイル不能になるため、`pub(crate) fn from_raw_parts(...)` を新設し、デコーダー内のみ検証済みフィールドで直接構築可能にする。

破壊的変更。`Request { ... }` の構造体リテラル構築、および直接フィールド代入 (`request.headers.push(...)` / `request.method = ...` / `request.uri = ...` / `request.body = ...`) は全箇所で禁止される。呼び出し側はすべて新 API に書き換える。

ブランチ名は CLAUDE.md「git ブランチの命名規則」に従い `feature/change-request-fields-private-with-validation` を使用する。

依存関係:
- 本 issue は `0017` (Response フィールド非公開化) と `0024` (StatusCode 型導入) の完了後に着手する
- `0017` で確立した「フィールド非公開化 + バリデーション + `from_raw_parts` + `#[non_exhaustive]`」のパターンを Request にそのまま適用する
- 本 issue 完了後、別 issue で `Method` 型 + `Request::with_method` を追加予定 (`0024` の Request 版)
- 本 issue 完了後、別 issue で Request 側の builder/mutator API 一貫化を追加予定 (`0021` の Request 版)

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

CRLF / NUL 注入は **HTTP Request Smuggling (CWE-444)** の温床で、特に reverse proxy 経路では致命的な脆弱性を生む。`Response` 側 (CWE-113 Response Splitting) よりも影響範囲が広い:

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

`pub struct Request { pub ... }` で全フィールド公開のため、将来 `trailers` 等を追加すると、構造体リテラル `Request { ... }` を使う全利用者がコンパイル不能になる。全フィールド非公開化により将来のフィールド追加は非破壊的になる。`#[non_exhaustive]` は同一 major バージョン内のフィールド追加が downstream のコンパイルを破壊しないことを型レベルで宣言する。

## 対応方針

### 0017 のパターンを踏襲

本 issue は Response 側 (`0017`) で確立したパターンを **そのまま Request に適用** する:

| Response (0017) | Request (本 issue) |
|---|---|
| `Response::new(code, reason)?` | `Request::new(method, uri)?` |
| `Response::with_version(v, c, r)?` | `Request::with_version(method, uri, version)?` |
| `pub(crate) fn Response::from_raw_parts(...)` | `pub(crate) fn Request::from_raw_parts(...)` |
| status_code バリデーション (`is_valid_status_code`) | method バリデーション (`is_valid_method`) |
| reason_phrase バリデーション (`is_valid_reason_phrase`) | uri バリデーション (`is_valid_request_target` 等) |
| 共通: version / header / non_exhaustive | 共通: version / header / non_exhaustive |

### 影響範囲一覧

| ファイル | 種別 | 内容 |
|---|---|---|
| `src/request.rs` | 主要変更 | フィールド非公開化、コンストラクタ `Result` 化、`pub(crate) from_raw_parts` 新設、アクセサ・setter 追加 |
| `src/validate.rs` | 修正 | method バリデーション (`is_valid_method`) を追加 (token 検証、既存の `is_valid_header_name` と同等)、URI バリデーション関数の整理 (`is_valid_request_target` を decoder 経路から共有可能にする) |
| `src/encoder.rs` | 修正 | Request 関連の全フィールド直接アクセスをアクセサ経由に書き換え、`encode()` doc 更新、二重バリデーション維持、`validate_request_fields` の追加 (Response 側と並行) |
| `src/decoder/request.rs` | 修正 | 構造体リテラル → `from_raw_parts` に書き換え |
| `src/error.rs` | 修正 | `EncodeError::InvalidMethod { method: String }` バリアントを追加 (既存の `InvalidUri` / `InvalidHeaderName` / `InvalidHeaderValue` / `InvalidVersion` は流用) |
| `examples/http11_client/src/main.rs` | 修正 | `Request::new(...)` / `add_header(...)` / `body = ...` を新 API に書き換え、関数シグネチャを `Result<Request, EncodeError>` 化 |
| `examples/http11_reverse_proxy/src/main.rs` | 修正 | upstream のヘッダーをループで `add_header` する箇所を `add_header(name, value)?` に変更し、関数シグネチャを `Result<Request, EncodeError>` 化する。`.unwrap()` は使わない (smuggling 防御の観点でサンプルが「お手本」となる必要があるため) |
| `examples/http11_server/src/main.rs` | 確認 | サーバー側は Request を主にデコードする側だが、テスト用に構築する箇所があれば書き換え |
| `examples/http11_server_io_uring/src/main.rs` | 確認 | 同上 |
| `tests/test_request.rs` | **新設** | バリデーションエラー再現テスト |
| `tests/test_encoder.rs` | 修正 | Request 構築の `.unwrap()` 追加、不正値テストの構築時エラー検証への書き換え |
| `pbt/tests/prop_request.rs` | 修正 | 全テストのフィールド直接アクセス → accessor に書き換え + バリデーション PBT 追加 |
| `pbt/tests/prop_encoder.rs` | 修正 | `Request::new()` / `.header()` に `.unwrap()` 追加 |
| `pbt/tests/prop_decoder/request.rs` | 修正 | フィールド直接アクセスをアクセサ経由に書き換え |
| `fuzz/fuzz_targets/fuzz_encode_request.rs` | 修正 | フィールド代入 → setter API に書き換え |
| `fuzz/fuzz_targets/fuzz_decoder_roundtrip.rs` | 修正 | 同上 (Request 側の構築箇所) |
| `fuzz/fuzz_targets/fuzz_request_response_helpers.rs` | 修正 | 同上 |
| `src/lib.rs` | 修正 | doctest の `Request::new()` / `.header()` に `.unwrap()` 追加 |
| `skills/shiguredo-http11/SKILL.md` | 修正 | Request の主要メソッド一覧を新 API に追従させ、サンプルコードに `.unwrap()` を追加 (smuggling 防御の観点でサンプルが「お手本」となる構造を強調) |

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
/// `header` / `add_header` / `set_header` 経由でのみ操作できる。`#[non_exhaustive]`
/// により、将来のフィールド追加 (例: `trailers`) は破壊的変更にならない。
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

#### コンストラクタの `Result` 化

```rust
impl Request {
    /// 新しいリクエストを作成 (HTTP/1.1)
    ///
    /// バリデーション順序: method → uri。
    /// 失敗時は最初に検出されたエラーを返す。
    ///
    /// `method` は RFC 9110 Section 9.1 の `method = token` (RFC 9110 Section 5.6.2) を要求する。
    /// `uri` は RFC 9112 Section 3.2 の request-target (origin-form / absolute-form /
    /// authority-form / asterisk-form) のいずれかであることを要求する。
    pub fn new(method: &str, uri: &str) -> Result<Self, EncodeError>

    /// カスタムバージョンでリクエストを作成
    ///
    /// バリデーション順序: method → uri → version。
    pub fn with_version(method: &str, uri: &str, version: &str) -> Result<Self, EncodeError>

    /// 検証済みの生フィールドから Request を構築 (デコーダー内部用)
    ///
    /// 0017 の Response::from_raw_parts と同じ設計。
    pub(crate) fn from_raw_parts(
        method: String,
        uri: String,
        version: String,
        headers: Vec<(String, String)>,
        body: Option<Vec<u8>>,
    ) -> Self
}
```

バリデーション呼び出し:

| コンストラクタ | 検証内容 | 使用関数 |
|---|---|---|
| `new` | method: token | `is_valid_method` (validate.rs に追加) |
| `new` | uri: request-target | `is_valid_request_target` (validate.rs から共有) |
| `with_version` | method: 同上 | 同上 |
| `with_version` | uri: 同上 | 同上 |
| `with_version` | version: token `/` DIGIT+ `.` DIGIT+ | `is_valid_protocol_version` |

### URI バリデーションのスコープ

本 issue では URI のバリデーションを **構文レベル (CRLF/NUL/SP の混入禁止 + ASCII 範囲チェック)** に留める。完全な RFC 3986 ABNF 準拠は別 issue で対応。

理由:
- 完全な RFC 3986 準拠は実装量が大きく、本 issue のスコープを超える
- 本 issue の主目的は smuggling 防御であり、CRLF/NUL の拒否で達成できる
- request-target form (origin / absolute / authority / asterisk) の判別は decoder 側で既に実装済み (`src/decoder/body.rs`)。encoder 側では構文レベルのバリデーションで十分

実装方針:
```rust
/// request-target が有効か確認 (構文レベル)
///
/// RFC 9112 Section 3.2 の request-target は CRLF/NUL/SP を含んではならない。
/// 完全な RFC 3986 ABNF 準拠は別 issue で対応する。
///
/// 本関数は smuggling 防御 (CWE-444) を目的とした最小限のバリデーション。
pub(crate) fn is_valid_request_target(target: &str) -> bool {
    !target.is_empty()
        && target.bytes().all(|b| matches!(b, 0x21..=0x7E))  // VCHAR のみ (SP/CTL/non-ASCII 拒否)
}
```

### ヘッダー追加 / 上書き / 取得 API

`0017` (Response) と完全に並行構造:

```rust
pub fn header(self, name: &str, value: &str) -> Result<Self, EncodeError>  // builder
pub fn add_header(&mut self, name: &str, value: &str) -> Result<(), EncodeError>  // mutator
pub fn set_header(&mut self, name: &str, value: &str) -> Result<(), EncodeError>  // 同名上書き
pub fn body(mut self, body: Vec<u8>) -> Self  // builder
```

バリデーションは `is_valid_header_name` / `is_valid_field_value` を使用 (`0017` で確立した validate.rs の関数を流用)。

### 読み取り専用アクセサ

```rust
pub fn method(&self) -> &str
pub fn uri(&self) -> &str
pub fn version(&self) -> &str
pub fn body(&self) -> Option<&[u8]>
```

`HttpHead` トレイト経由のメソッド (`headers`, `get_header`, `get_headers`, `has_header`, `connection`, `is_keep_alive`, `content_length`, `is_chunked`) は既存通り提供。

### encoder 側の `validate_request_fields`

`src/encoder.rs` に Response の `validate_response_fields` と並行する `validate_request_fields` を追加 (既存の Request encoder ロジックに散らばっている検証を集約)。`0017` の方針に従い:

- 構築時バリデーション (本 issue で追加) と encode 時バリデーション (`validate_request_fields`) の二重チェックを維持
- 構築時バリデーションを通過した Request は `validate_request_fields` の各検証分岐を必ず通過するため、`from_raw_parts` 経由のテストでカバレッジ補填する

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
| `request.body = None` | body None 復帰は別 issue (Request 版 0021) で対応 |
| `request.method` (参照) | `request.method()` |
| `request.uri` (参照) | `request.uri()` |
| `request.version` (参照) | `request.version()` |
| `request.headers.len()` (参照) | `HttpHead::headers(&request).len()` |
| `request.body.as_deref()` | `request.body()` |

`Request { ... }` 構造体リテラルの直接構築は全箇所で禁止される。crate 内は `from_raw_parts` を使用する。

### examples の改修方針

CLAUDE.md「サンプルは **お手本**」原則に従い、examples は **`.unwrap()` を使わず `?` 伝播する** 設計に書き換える。特に reverse proxy は smuggling 防御のショーケースとして、動的入力に対して必ずバリデーションを通すサンプルにする。

- `build_request` 等の戻り値を `Result<Request, EncodeError>` に変更し、`?` で伝播
- 静的リテラルのヘッダー (`"Host"`, `"User-Agent"` 等) は `.expect("static header is valid")` を許容するが、メッセージで「リテラルが妥当だから expect」を明示
- 動的入力 (upstream のヘッダー値、URI パラメータ等) には必ず `?` を使う

## CHANGES.md

`## develop` セクションに以下を追加する:

```
- [CHANGE] `Request` の全フィールドを非公開化し、構築時バリデーションを追加する
  - 構築は `Request::new` / `Request::with_version` が `Result<Self, EncodeError>` を返す形に変更する
  - `add_header` / `header` でヘッダー名 (RFC 9110 Section 5.1 token) と値 (RFC 9110 Section 5.5 field-value, CR/LF/NUL 不可) をバリデートし `Result` を返す
  - `set_header` を新設し、同名ヘッダーの上書きを可能にする
  - method を RFC 9110 Section 9.1 token として、URI を構文レベル (CRLF/NUL/SP 禁止) でバリデートする
  - HTTP Request Smuggling (CWE-444) 防御を強化する
  - `pub(crate) fn from_raw_parts` を新設し、デコーダー内部からの検証済み構築を可能にする
  - `method()` / `uri()` / `version()` / `body()` の読み取り専用アクセサを追加する
  - 構造体に `#[non_exhaustive]` を付与する
  - `encoder.rs` 内部の全フィールド直接アクセスをアクセサ経由に書き換える
  - @voluntas
- [ADD] `EncodeError::InvalidMethod` バリアントを追加する
  - @voluntas
```

## 検証方針

### 不変条件が構築時点で守られることの確認

新規単体テスト (`tests/test_request.rs`) で以下を検証する:

- 不正な method (空文字列、CRLF を含む、SP を含む、token 違反文字を含む) で `Err(InvalidMethod)` が返る
- 不正な URI (空文字列、CRLF を含む、NUL を含む、SP を含む) で `Err(InvalidUri)` が返る
- スペースを含むヘッダー名で `Err(InvalidHeaderName)` が返る
- CRLF を含むヘッダー値で `Err(InvalidHeaderValue)` が返る
- NUL を含むヘッダー値で `Err(InvalidHeaderValue)` が返る
- 空ヘッダー値が合法であること (RFC 9110 Section 5.5: `field-value = *field-content`)
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
    EncodeError::InvalidUri { .. }
));
```

### 既存挙動が回帰しないことの確認

- 既存の単体テスト (`tests/test_encoder.rs` 等) が新 API に追従して green になる
- PBT (`prop_request.rs`, `prop_encoder.rs`, `prop_decoder/request.rs` 等) が新 API に追従して green になる
- fuzz ターゲット (`fuzz_encode_request`, `fuzz_decoder_roundtrip`, `fuzz_request_response_helpers`) が新 API に追従して green になる
- 全 examples (`http11_server`, `http11_server_io_uring`, `http11_reverse_proxy`, `http11_client`) がコンパイルおよび実行可能である

### カバレッジ検証

```bash
cargo llvm-cov clean --workspace
cargo llvm-cov --no-report -p shiguredo_http11 --lib -- request validate
cargo llvm-cov --no-report -p shiguredo_http11 --lib -- encoder
cargo llvm-cov --no-report -p shiguredo_http11 --test test_request
cargo llvm-cov --no-report -p pbt --test prop_request
cargo llvm-cov report
```

`Request::new` / `with_version` / `add_header` / `header` / `set_header` / `from_raw_parts` の全バリデーション分岐 (成功パス・失敗パス) がカバーされていることを確認する。

## 受け入れ基準

- ブランチ名が `feature/change-request-fields-private-with-validation` であること
- `make fmt && make clippy && make check && make test` がすべて成功する
- `src/request.rs` から `pub method` / `pub uri` / `pub version` / `pub headers` / `pub body` が消えている
- `Request` 構造体に `#[non_exhaustive]` が付いている
- `pub(crate) fn from_raw_parts` が Request に存在し、decoder がこれを使用している
- バリデーションエラー再現テストが全種成功する
  - `InvalidMethod` (空文字列, CRLF 含む, SP 含む, token 違反)
  - `InvalidUri` (空文字列, CRLF 含む, NUL 含む, SP 含む)
  - `InvalidHeaderName` (スペース含む, 空文字列)
  - `InvalidHeaderValue` (CRLF 含む, LF 含む, NUL 含む)
  - `InvalidVersion` (不正形式)
- 空ヘッダー値が合法であることのテストが存在する
- `set_header` の上書き動作が検証されている
- Request smuggling ペイロード (CRLF 注入による TE/CL 偽装、method/URI への CRLF 注入) を構築時に拒否することのテストが存在する
- `encoder.rs` 内部で `request.method` / `request.uri` / `request.version` / `request.headers` / `request.body` への直接フィールドアクセスが残っていない
- `cargo llvm-cov` でコンストラクタ / `add_header` / `set_header` / `from_raw_parts` のバリデーション分岐がカバーされている
- 全 fuzz ターゲットが新 API に追従しコンパイル可能である
- `set_header` のバリデーション失敗時に既存ヘッダーが消えない (アトミック性) ことが単体テストで検証されている
- 全 examples が `.unwrap()` を使わず `?` 伝播で書かれている (静的リテラルへの `.expect("...")` は理由付きで許容)
- `encoder.rs` 内の `validate_request_fields` 各検証分岐が `from_raw_parts` 経由のテストでカバーされている
- `skills/shiguredo-http11/SKILL.md` の Request API 一覧およびサンプルコードが新 API に追従している
- `EncodeError::InvalidMethod` バリアントが追加されている
- `from_raw_parts` の不変条件が `debug_assert!` で検査されており、debug ビルドで契約破りを検出可能であること
