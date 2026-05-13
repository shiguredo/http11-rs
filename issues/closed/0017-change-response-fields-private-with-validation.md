# 0017: Response のフィールドを非公開化しバリデート付き構築に統一する

Created: 2026-05-06
Completed: 2026-05-06
Model: Opus 4.7

## 概要

`Response` 構造体の全フィールド (`version`, `status_code`, `reason_phrase`, `headers`, `body`, `omit_body`) を非公開化し、バリデート付きコンストラクタと setter API による構築に統一する。構造体には `#[non_exhaustive]` を付与し、将来のフィールド追加による破壊的変更を防ぐ。

`src/decoder/response.rs` の構造体リテラル構築はフィールド非公開化でコンパイル不能になるため、`pub(crate) fn from_raw_parts(...)` を新設し、デコーダー内のみ検証済みフィールドで直接構築可能にする。合わせて decoder 側の `is_valid_reason_phrase` 呼び出しを、空文字列（reason-phrase absent）を許容する形に修正する。

破壊的変更。`Response { ... }` の構造体リテラル構築、および直接フィールド代入 (`response.headers.push(...)` / `response.status_code = ...` / `response.body = ...` / `response.omit_body = ...`) は全箇所で禁止される。呼び出し側はすべて新 API に書き換える。

ブランチ名は CLAUDE.md「git ブランチの命名規則」に従い `feature/change-response-fields-private-with-validation` を使用する。

依存関係:
- 本 issue は 0020 (StatusClass enum) より先に実施する。0020 は `self.status_code` に直接アクセスしているため、本 issue 完了後に getter `self.status_code()` 経由に追従する必要がある。
- 0021 (ビルダー / mutator API 一貫化) は本 issue 完了後に着手する。0021 で `add_header` の戻り値を `Result<&mut Self, EncodeError>` にする予定。本 issue では 0021 のシグネチャ変更を先取りせず、`Result<(), EncodeError>` で導入する (チェイン化は 0021 の責務)。
- pending/0018 (`omit_body` を encoder 側へ移譲) は本 issue で `omit_body` フィールドを存続させる。0018 が将来 accept された場合は、本 issue で追加する `is_body_omitted()` getter および `omit_body(bool)` ビルダーは 0018 の側で撤去する。
- 注: `issues/0023-enhance-response-doc-rfc-references.md` が `is_keep_alive` 等の doc 補強を提案している。本 issue を先に実施し、0023 はその後に適用する。
- 注: 0021 は `body` の getter/builder 同名併存を「Rust では同一 impl ブロック内で同名メソッドを定義できない」と誤認し getter を `as_body` に改名する計画を立てている。Rust では `&self` / `mut self` でシグネチャが異なるため同名併存は可能であり、0017 の設計で問題ない。0021 着手時にこの点を確認すること。
- 注: 0020 側の issue 本文には「本 issue (0020) に依存関係はない。独立して着手可能」と記載されているが、0020 の `self.status_code` への直接アクセスは 0017 のフィールド非公開化でコンパイル不能になる。0020 を先に着手した場合は、0017 実装時に 0020 のコードを getter `self.status_code()` 経由に追従修正する必要がある。

## 根拠

### 問題 1: 構築後に外部から不変条件を破壊できる

現状すべてのフィールドが `pub` のため、構築後に以下のような不正値の代入が可能:

- `response.status_code = 600` (RFC 9110 Section 15: ステータスコード範囲外)
- `response.reason_phrase = "OK\r\nX-Inject: y".to_string()` (CRLF 注入 — HTTP レスポンス分割攻撃の温床)
- `response.headers.push(("Bad Name".to_string(), "x".to_string()))` (token 違反: 空白混入)
- `response.headers.push(("X-Header".to_string(), "x\0y".to_string()))` (NUL 混入 — RFC 9110 Section 5.5: "Field values containing CR, LF, or NUL characters are invalid and dangerous")
- `response.version = "garbage".to_string()` (不正バージョン)
- `response.body = Some(vec![b'x'])` (body を勝手に差し替えられる)
- `response.omit_body = true` (body 送信抑止フラグを勝手に変更できる)

CRLF / NUL 注入は HTTP レスポンス分割攻撃 (HTTP Response Splitting) の温床で、実害のある脆弱性。CLAUDE.md「性能より堅牢性を優先」「一切妥協しない」に正面から反する。

### 問題 2: バリデーションが encoder 任せに遅延している

構築時バリデーションが皆無で、エラーが `encode_response` 実行時まで遅延される。本来は構築時点で検出可能な不正値が、ボディを書き込もうとする段階まで持ち越される。`Result` を返す構築 API なら、利用側のエラーハンドリングが「構築時」に集約され、検出が早くなる。

### 問題 3: ヘッダーの case-insensitive 重複が野放し

HTTP ヘッダー名は case-insensitive (RFC 9110 Section 5.1) だが、`add_header("Content-Type", ...)` と `add_header("content-type", ...)` を両方追加できてしまう。get 側は `HttpHead` 経由で case-insensitive に検索されるのに、保存は raw のまま — 非対称で利用者の混乱を招く。本 issue では保存側の正規化は行わず (**別 issue で対応**)、「検索のみ case-insensitive」の現状を維持しつつ、**入力時の token / 値バリデーションを追加する範囲に留める**。

### 問題 4: フィールド追加が破壊的変更になる

`pub struct Response { pub ... }` で全フィールド公開のため、将来 `trailers` 等を追加すると、構造体リテラル `Response { ... }` を使う全利用者がコンパイル不能になる。全フィールド非公開化により将来のフィールド追加は非破壊的になる。`#[non_exhaustive]` は同一 major バージョン内のフィールド追加が downstream のコンパイルを破壊しないことを型レベルで宣言する。

## 対応方針

### 影響範囲一覧

| ファイル | 種別 | 内容 |
|---|---|---|
| `src/response.rs` | 主要変更 | フィールド非公開化、コンストラクタ `Result` 化、`pub(crate) from_raw_parts` 新設、アクセサ・setter 追加 |
| `src/validate.rs` | 修正 | `is_valid_reason_phrase` に空文字列拒否を追加、decoder 側で空文字列を absent として扱うよう対応 |
| `src/encoder.rs` | 修正 | 全フィールド直接アクセスをアクセサ経由に書き換え、`encode()` doc 更新、二重バリデーション維持、`validate_response_fields` の reason-phrase 検証で空文字列を absent としてスキップ、`capacity_tests` モジュール内の `Response::new()` 呼び出しに `.unwrap()` 追加、`validate_response_fields` 分岐のカバレッジ補填テスト（`from_raw_parts` 経由）を `#[cfg(test)]` に追加 |
| `src/decoder/response.rs` | 修正 | 構造体リテラル → `from_raw_parts` に書き換え、空 reason-phrase 許容対応 |
| `src/error.rs` | 変更不要 | 必要なバリアントは全て既存。追加不要。 |
| `examples/http11_server/src/main.rs` | 修正 | `build_response` / `build_compressed_response` / `add_connection_headers` の戻り値を `Result<Response, EncodeError>` に変更し、`.header(...)` / `.add_header(...)` 連鎖を `?` で繋ぐ。呼び出し元（`serve_request`）も `?` 伝播対応（`EncodeError: std::error::Error` を `Box<dyn Error>` に `From` 変換）。実装前に `rg "Response::new\|\.header\|\.add_header\|\.headers\.push\|body\s*=" src/examples/http11_server/src/main.rs` で全操作箇所を特定すること |
| `examples/http11_server_io_uring/src/main.rs` | 修正 | 同上 |
| `examples/http11_reverse_proxy/src/main.rs` | 修正 | upstream の任意ヘッダーをループで `add_header` する箇所（`response_for_headers.add_header(...)` 4 箇所）を `add_header(name, value)?` に変更し、関数シグネチャを `Result<Response, EncodeError>` 化する。`.unwrap()` は使わない（任意入力での panic は CLAUDE.md「サンプルはお手本」原則違反）。あらかじめ `rg "add_header\|\.headers\.push\|body\s*=" src/examples/http11_reverse_proxy/src/main.rs` で全操作箇所を特定してから修正に着手すること |
| `examples/http11_client/src/main.rs` | 修正 | 構造体リテラル（line 216-226, 339-349）を `Response::with_version(...)?` + `add_header(...)?` で再構築する形に書き換え。受信した raw 値が再検証で失敗した場合は呼び出し元で `?` 伝播。`print_response()` 内のフィールドアクセス（line 354-356, 360, **365-369**, 373）を accessor 経由に書き換え。特に line 365-369 の `response.headers.iter().find(...)` は `HttpHead::headers(&response).iter().find(...)` に書き換え |
| `tests/test_encoder.rs` | 修正 | **正常系**: `Response::new()` 等の呼び出しに `.unwrap()` 追加。**異常系**: 不正値をエンコード時検出に頼っていたテスト（`test_encode_response_invalid_version`, `test_encode_response_invalid_status_code`, `test_encode_response_crlf_in_reason_phrase`, `test_encode_response_crlf_in_header_name`, `test_encode_response_crlf_in_header_value`, NUL ヘッダー値テスト等）は、不正値が `Response::new` / `with_version` / `header` の構築時点で `Err` になるため `.unwrap()` では対応不可。これらのテストは構築時エラーの直接検証に書き換える。エンコード時バリデーション分岐のカバレッジは `encoder.rs` の `#[cfg(test)]` モジュールで `from_raw_parts` 経由テストにより補填する（後述） |
| `tests/test_response.rs` | **新設** | バリデーションエラー再現テスト |
| `pbt/tests/prop_response.rs` | 修正 | 全テストのフィールド直接アクセス → accessor に書き換え + バリデーション PBT 追加 |
| `pbt/tests/prop_encoder.rs` | 修正 | `Response::new()` / `.header()` に `.unwrap()` 追加 |
| `pbt/tests/prop_decoder/response.rs` | 修正 | `Response::new()` 等に `.unwrap()` 追加。フィールド直接アクセス（`response.status_code`, `response.body`, `response.reason_phrase`, `response.headers`, `response.version`）をアクセサ経由に書き換え（`response.status_code()`, `response.body()`, `response.reason_phrase()`, `HttpHead::headers(&response)`, `HttpHead::version(&response)`） |
| `fuzz/fuzz_targets/fuzz_encode_response.rs` | 修正 | フィールド代入 → setter API に書き換え、`body_present` 分岐で `None` 復元 |
| `fuzz/fuzz_targets/fuzz_decoder_roundtrip.rs` | 修正 | フィールド代入 → setter API に書き換え |
| `fuzz/fuzz_targets/fuzz_request_response_helpers.rs` | 修正 | 同上 |
| `fuzz/fuzz_targets/fuzz_decoder_chunked.rs` | 修正 | `Response::new()` / `add_header()` に `.unwrap()` 追加 |
| `src/lib.rs` | 修正 | doctest の `Response::new().header()` に `.unwrap()` 追加 |
| `tests/test_decoder.rs:1359-1360` | 修正 | `response.status_code` / `response.body.as_deref()` を accessor 経由に書き換え |
| `tests/test_decode_body.rs:237` | 修正 | `response.body.as_deref()` を `response.body()` に書き換え |
| `pbt/tests/prop_decoder/body.rs` | 修正 | `response.body`, `response.status_code` を含む全フィールド直接アクセスをアクセサ経由に書き換え |
| `pbt/tests/proptest-regressions/response.txt` | 確認 | 既存 2 シード (`status_codes = [100, 300]` / `status = 205, reason = "OK", body_data = []`) の再現可能性を確認する。本 issue で strategy 関数のシグネチャは変更せず、内部の `Response { ... }` リテラル構築を `Response::with_version(...).unwrap()` に置き換えるだけのため、生成される値は同等で seed は再現可能。削除不要。新規 PBT で発見された失敗ケースは新シードとして追加される (正常動作) |
| `skills/shiguredo-http11/SKILL.md` | 修正 | Response の主要メソッド一覧 (line 36) を新 API (`new` / `with_version` / `add_header` / `set_header` / `is_body_omitted` 等が `Result` 化または新設) に追従させ、サンプルコードに `.unwrap()` を追加する。合わせて既存漏れの `is_informational()` も追記する |

### src/response.rs

#### フィールド非公開化と `#[non_exhaustive]`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Response {
    version: String,
    status_code: u16,
    reason_phrase: String,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
    omit_body: bool,
}
```

全フィールドの `pub` を除去し非公開化する。`#[non_exhaustive]` は同一 major バージョン内のフィールド追加（例: `trailers` 等）が downstream のコンパイルを破壊しないことを型レベルで宣言するためのものである。注: フィールド非公開化後は外部クレートが構造体パターンマッチを行うことはそもそも不可能（プライベートフィールドを含む構造体はパターン分解できない）であり、`#[non_exhaustive]` の効能は「将来フィールドが追加されても外部クレートからの構造体リテラル構築が破壊されないこと」に限定される。

#### デコーダー用 `pub(crate)` コンストラクタ (新設)

```rust
/// 検証済みの生フィールドから Response を構築 (デコーダー内部用)
///
/// デコーダー側で既にバリデーション済みのフィールドを直接受け取る。
/// コンストラクタのバリデーションはスキップする。
/// 外部クレートからはアクセス不可 (`pub(crate)`)。
///
/// # 不変条件 (呼び出し側の責務)
///
/// 呼び出し側 (decoder) は以下の不変条件をすべて満たすフィールドのみを渡すこと:
/// - `version`: `is_valid_protocol_version` を通過済み
/// - `status_code`: `is_valid_status_code` を通過済み (RFC 9110 §15: 100..=599)
/// - `reason_phrase`: 空文字列 (RFC 9112 §4: reason-phrase absent) または
///   `is_valid_reason_phrase` を通過済み
/// - `headers`: 各エントリが `is_valid_header_name` / `is_valid_field_value` を通過済み
///
/// `omit_body` は受信側 Response では常に `false` に固定する (`omit_body` は
/// 送信側専用フラグであり、HEAD レスポンス受信時の「body なし」状態は
/// `body == None` で表現される)。
///
/// 引数は所有値 (`String` / `Vec`) を受け取る。decoder 側 (`ResponseHead`) が
/// 所有値を保持しているため、move による zero-copy 構築が可能 (Rust API
/// ガイドライン C-OWNED-PARAMETERS に沿う)。
///
/// 注: 命名は標準ライブラリの unsafe 慣習 (`Vec::from_raw_parts` 等) と表面的に
/// 衝突するが、本関数は unsafe ではない。`pub(crate)` のため外部公開 API には
/// 影響しない。crate 内で命名衝突が問題になった場合は、別 issue で
/// `from_decoded_parts` 等への改名を検討する。
pub(crate) fn from_raw_parts(
    version: String,
    status_code: u16,
    reason_phrase: String,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
) -> Self {
    // debug ビルドのみで契約を検査する。release では検証スキップ (decoder 経路の最適化)。
    // 契約違反は decoder のバグであり、release で発覚した場合は encoder 側の
    // 二重バリデーション (`validate_response_fields`) が最後の防御線となる。
    debug_assert!(
        crate::validate::is_valid_protocol_version(&version),
        "from_raw_parts: invalid version: {version:?}"
    );
    debug_assert!(
        crate::validate::is_valid_status_code(status_code),
        "from_raw_parts: invalid status_code: {status_code}"
    );
    debug_assert!(
        reason_phrase.is_empty()
            || crate::validate::is_valid_reason_phrase(&reason_phrase),
        "from_raw_parts: invalid reason_phrase: {reason_phrase:?}"
    );
    debug_assert!(
        headers.iter().all(|(n, v)| {
            crate::validate::is_valid_header_name(n)
                && crate::validate::is_valid_field_value(v)
        }),
        "from_raw_parts: invalid header(s)"
    );
    Self { version, status_code, reason_phrase, headers, body, omit_body: false }
}
```

`src/decoder/response.rs:805-812` の構造体リテラルはこのコンストラクタに書き換える。

#### コンストラクタの `Result` 化

```rust
/// 新しいレスポンスを作成 (HTTP/1.1)
///
/// バリデーション順序: status_code → reason_phrase。
/// 失敗時は最初に検出されたエラーを返す。
///
/// `status_code` は RFC 9110 §15 (100..=599) を要求する
/// (将来 RFC が範囲を改訂する可能性あり)。
/// `reason_phrase` は RFC 9112 §4 の `1*( HTAB / SP / VCHAR / obs-text )` を要求する
/// (空不可、本 API は送信側専用ポリシー)。
///
/// version は `"HTTP/1.1"` 固定のため、`is_valid_protocol_version` は呼び出さない
/// (固定値が常に検証を通過するため)。
///
/// # 引数の文字集合制限 (既存の API 制限を継承)
///
/// `reason_phrase: &str` は Rust の `&str` (UTF-8 不変条件) を要求するため、
/// RFC 9112 §4 の `obs-text = %x80-FF` のうち UTF-8 として valid なシーケンスのみ
/// を表現可能。任意バイト列の obs-text を渡す API は本 issue では提供しない
/// (validate.rs:140-145 の既存制限を継承)。
pub fn new(status_code: u16, reason_phrase: &str) -> Result<Self, EncodeError>

/// カスタムバージョンでレスポンスを作成
///
/// バリデーション順序: version → status_code → reason_phrase。
/// 失敗時は最初に検出されたエラーを返す。
///
/// `version` は `is_valid_protocol_version` (`token "/" DIGIT+ "." DIGIT+`) で検証する。
///
/// # RFC との乖離
///
/// 本 API は RFC 9112 §2.3 の `HTTP-name = %s"HTTP"` (case-sensitive) を強制せず、
/// token として大文字小文字を許容する緩和形式を採用する。これは validate.rs の
/// `is_valid_protocol_version` の既存方針 (RTSP 等の互換のため token を許容) を
/// 継承するものであり、HTTP として送信する場合は呼び出し側が
/// `"HTTP/1.1"` を渡す責務がある。
/// 注: DIGIT+ (1 桁以上) は RFC 7826 Section 20.3 の RTSP 対応のための拡張であり、
/// RFC 9112 §2.3 の `DIGIT "." DIGIT` (各 1 桁) より広い。
pub fn with_version(version: &str, status_code: u16, reason_phrase: &str) -> Result<Self, EncodeError>
```

バリデーション呼び出し:

| コンストラクタ | 検証内容 | 使用関数 |
|---|---|---|
| `new` | status_code: 100..=599 | `is_valid_status_code` (`validate.rs:130`) |
| `new` | reason_phrase: 空不可 + VCHAR/HTAB/SP/obs-text | `is_valid_reason_phrase` (`validate.rs:146`) |
| `with_version` | version: token `/` DIGIT+ `.` DIGIT+ | `is_valid_protocol_version` (`validate.rs:85`) |
| `with_version` | status_code: 100..=599 | `is_valid_status_code` |
| `with_version` | reason_phrase: 空不可 + VCHAR/HTAB/SP/obs-text | `is_valid_reason_phrase` |

#### ヘッダー追加 API のバリデート

```rust
/// ヘッダーを追加 (ビルダーパターン)
///
/// 名前は RFC 9110 §5.1 の field-name = token (1*tchar、RFC 9110 §5.6.2)、
/// 値は RFC 9110 §5.5 の field-value を満たす必要がある。
/// CR/LF/NUL は RFC 9110 §5.5 で「invalid and dangerous」と明示され、
/// MUST either reject or replace と定義されているため拒否する。
///
/// # 値の文字集合制限 (既存の API 制限を継承)
///
/// `value: &str` は UTF-8 不変条件を持つため、RFC 9110 §5.5 の
/// `obs-text = %x80-FF` のうち UTF-8 として valid なシーケンスのみ
/// 表現可能 (validate.rs:50-59 の既存制限を継承)。
pub fn header(self, name: &str, value: &str) -> Result<Self, EncodeError>

/// ヘッダーを追加
///
/// 戻り値は `Result<(), EncodeError>`。チェイン化 (`Result<&mut Self, EncodeError>`) は
/// issue 0021 で導入予定のため、本 issue では先取りしない。
pub fn add_header(&mut self, name: &str, value: &str) -> Result<(), EncodeError>
```

バリデーション: 名前は `is_valid_header_name` (`validate.rs:17`)、値は `is_valid_field_value` (`validate.rs:60`) を使用する。`is_valid_field_value` は 0x09, 0x20-0x7E, 0x80-0xFF のみを許可し、CR (0x0D), LF (0x0A), NUL (0x00) を許可文字集合から除外することで CRLF/NUL 注入を拒否する。空文字列は RFC 9110 Section 5.5 の `field-value = *field-content` に従い合法であり、`is_valid_field_value` は空文字列を拒否しない (現状維持)。

注: `is_valid_field_value` はバイトレベルの文字集合検証のみで、RFC 9110 §5.5 の `field-content` 構造制約（VCHAR/obs-text で開始し終了する、先頭/末尾の空白禁止）の検証は未実装。CTL 全般を拒否するのは RFC の MAY retain より厳格だが、HTTP 応答分割攻撃のリスクを踏まえた堅牢性優先の判断である。先頭/末尾空白の構造制約は別 issue で対応する。

#### ヘッダー上書き用 API (新設)

```rust
/// 指定した名前の既存ヘッダーを全削除し、新規に追加する
///
/// 同名 (case-insensitive) のヘッダーをすべて削除した後、
/// 指定した name/value で新規追加する。呼び出し後、対象ヘッダーは末尾に位置する
/// (元の位置は保存しない)。
///
/// バリデーションが失敗した場合は既存ヘッダーは変更されない (アトミック性の保証)。
///
/// 注: Set-Cookie のように同名複数値が意味を持つヘッダーには使ってはならない。
/// その場合は `add_header` を使うこと (RFC 6265 など)。
pub fn set_header(&mut self, name: &str, value: &str) -> Result<(), EncodeError>
```

既存の `add_header` だけではヘッダー値の修正手段がなく、間違った値を追加した場合に Response 再構築が必要になっていた。本 API でこのギャップを埋める。

実装方針 (アトミック性確保):
1. `is_valid_header_name(name) && is_valid_field_value(value)` を **先に** 検証し、失敗時は早期 `Err` で `self` を変更しない
2. バリデーション通過後に `self.headers.retain(|(n, _)| !n.eq_ignore_ascii_case(name))` で同名全削除
3. `self.headers.push((name.to_string(), value.to_string()))` で末尾追加
4. `Ok(())` を返す

`add_header` を内部で呼び出してしまうと、retain 実行後に add_header のバリデーションが失敗して既存ヘッダーが消えた状態で `Err` が返るためアトミック性が崩れる。アトミック性確保のため、`add_header` を流用せず手書きで実装する。

#### 読み取り専用アクセサ

```rust
pub fn version(&self) -> &str
pub fn status_code(&self) -> u16
pub fn reason_phrase(&self) -> &str
pub fn body(&self) -> Option<&[u8]>
pub fn is_body_omitted(&self) -> bool   // omit_body フィールドの getter (命名は is_ プレフィックスで bool を明示)
```

`version()` は `status_code()` / `reason_phrase()` / `body()` との非対称を避けるため、`HttpHead` トレイト経由の間接アクセスに加えて `Response` の固有メソッドとしても提供する。実装は `self.version.as_str()` を返す。`HttpHead::version(&self)` の実装はそのまま残す（トレイト実装の義務）。

以下は既存の `HttpHead` トレイト実装経由で提供済みのため追加不要:
- `version()` → `HttpHead::version` → `&str`
- `headers()` → `HttpHead::headers` → `&[(String, String)]`
- `get_header()` → `HttpHead::get_header` → `Option<&str>`
- `get_headers()` → `HttpHead::get_headers` → `Vec<&str>`
- `has_header()` → `HttpHead::has_header` → `bool`
- `connection()` → `HttpHead::connection` → `Option<&str>`
- `content_length()` → `HttpHead::content_length` → `Option<u64>`

#### ビルダーメソッドの非 Result 維持

```rust
pub fn body(mut self, body: Vec<u8>) -> Self     // 変更なし、常に Some(body) を設定
pub fn omit_body(mut self, omit: bool) -> Self   // 変更なし
```

`body()` は `Vec<u8>` を取るため構文上の不正値はなく、`omit_body()` は `bool` を取るため同様。両者とも `Self` を返し続け、builder chain で `?` が不要な末端メソッドとして機能させる。

注: `body()` は常に `Some(body)` を設定するため、一度設定すると `None` に戻せない。body の削除 (`None` 復帰) は別 issue (0021) で `without_body` として対応する。

#### `body()` builder と `body()` getter の同名併存

本 issue では builder `pub fn body(mut self, body: Vec<u8>) -> Self` と getter `pub fn body(&self) -> Option<&[u8]>` が **同名** で併存する。Rust の inherent impl は `&self` / `mut self` でシグネチャが異なるため同名併存は可能であり、`add_header` と `header` や `set_header` のような既存の命名規則との非対称が生じるが、両者は受容する。

判断: 以下の理由から同名併存を本 issue では維持する:
1. `body(data)` builder は既に確立した公開 API であり、改名すると本 issue の影響範囲がさらに拡大する
2. `0021` では getter を `as_body` に改名する計画があるが、それは誤認に基づく（同名併存は Rust の言語仕様上合法）。`0021` 着手時に 0017 の同名併存判断を確認すること
3. `add_header`（mutator）/ `header`（builder）のような命名分離パターンは将来的な整理候補だが、破壊的変更の回数を減らすため本 issue では先延ばしにする

#### ヘッダー重複に関する方針

- `add_header` / `header` は同名複数値を許す (Set-Cookie 等の用途で必須)
- `set_header` は同名全削除後に追加 (上書き)
- 保存は raw (case-preserving)、検索は case-insensitive (現状維持)
- 正規化 (lowercase 強制等) は別 issue で対応

### src/validate.rs

#### `is_valid_reason_phrase` の修正

```rust
/// reason-phrase が有効か確認 (RFC 9112 Section 4)
///
/// reason-phrase = 1*( HTAB / SP / VCHAR / obs-text )
///
/// 空文字列は RFC 9112 Section 4 の status-line ABNF で reason-phrase が
/// absent (未指定) の場合に発生するが、本関数は「reason-phrase が指定された場合」
/// の文字集合検証を意図している。空文字列の扱いは呼び出し側の責務とする。
pub(crate) fn is_valid_reason_phrase(phrase: &str) -> bool {
    !phrase.is_empty() && phrase.bytes().all(|b| matches!(b, 0x09 | 0x20..=0x7E | 0x80..=0xFF))
}
```

現状は `.bytes().all(...)` のみで空文字列が許容されていた (`.all()` は空イテレータで `true`)。RFC は `1*(...)` と最低 1 文字を要求するため、`!phrase.is_empty()` を追加する。これは RFC 9112 Section 4 の status-line ABNF における `[ reason-phrase ]` (OPTIONAL) の absent ケースとは別の扱いであり、absent (= 空文字列) は呼び出し側でスキップする。

#### decoder 側の空 reason-phrase 対応

`src/decoder/response.rs` の `is_valid_reason_phrase` 呼び出し箇所を以下のように修正する:

```rust
// 変更前
if !is_valid_reason_phrase(&reason_phrase) { ... }

// 変更後: 空文字列は RFC 9112 Section 4 の reason-phrase absent 相当
if !reason_phrase.is_empty() && !is_valid_reason_phrase(&reason_phrase) {
    return Err(Error::InvalidData("invalid reason-phrase".to_string()));
}
```

これにより、RFC 準拠の `HTTP/1.1 200 \r\n` (reason-phrase absent) を正しく受理しつつ、指定された reason-phrase の文字集合は適切に検証する。

#### encoder 側の空 reason-phrase 対応 (重要)

reverse proxy 等で「decoder が受理した空 reason_phrase の Response を encoder で再送信する」経路を破壊しないため、encoder 側の `validate_response_fields` (`src/encoder.rs:378-383`) も同様に空文字列をスキップする形に修正する:

```rust
// 変更前
if !is_valid_reason_phrase(&response.reason_phrase) {
    return Err(EncodeError::InvalidReasonPhrase {
        phrase: response.reason_phrase.clone(),
    });
}

// 変更後: reason-phrase absent (空文字列) の場合は文字集合検証をスキップ
// (RFC 9112 §4: server MUST send the space ... even when the reason-phrase is absent)
if !response.reason_phrase.is_empty()
    && !is_valid_reason_phrase(&response.reason_phrase)
{
    return Err(EncodeError::InvalidReasonPhrase {
        phrase: response.reason_phrase.clone(),
    });
}
```

これにより以下の経路が成立する:
1. reverse proxy が upstream から `HTTP/1.1 200 \r\n` (reason-phrase absent) を受信
2. decoder が `from_raw_parts(version, 200, "", headers, body)` で空 `reason_phrase` を持つ Response を構築
3. proxy が `encode_response(&response)` で downstream へ転送
4. encoder の `validate_response_fields` が空文字列をスキップして送信成功

注: 構築時 (`Response::new` / `with_version`) は引き続き空文字列を `Err(InvalidReasonPhrase)` で拒否する (送信側ポリシー、API 利用者の利便性のため)。`from_raw_parts` 経由でのみ空 reason_phrase を持つ Response が生成され、encoder はそれを正しく送信できる。

### src/encoder.rs

#### 内部フィールド参照のアクセサ化

フィールド非公開化に伴い、`encoder.rs` 内の全 `Response` フィールド直接アクセスをアクセサメソッド経由に書き換える。主な変更箇所:

| 種別 | 旧コード | 新コード | 注意 |
|---|---|---|---|
| status_code | `response.status_code` | `response.status_code()` | — |
| reason_phrase | `&response.reason_phrase` / `response.reason_phrase.as_bytes()` | `response.reason_phrase()` / `.as_bytes()` | `.clone()` は `.to_string()` に変更（`&str::clone()` は `&str` を返すため `String` が必要なエラー構築箇所では `.to_string()` が必須） |
| version | `&response.version` / `response.version.as_bytes()` | `HttpHead::version(response)` / `.as_bytes()` | 同上 |
| headers | `&response.headers` | `HttpHead::headers(response)` | — |
| body 長 | `response.body.as_deref().map(...)` | `response.body().map(...)` | — |
| body データ | `response.body.as_deref()` | `response.body()` | — |
| omit_body | `response.omit_body` | `response.is_body_omitted()` | — |

影響を受ける関数: `should_auto_emit_content_length_for_response`, `estimate_response_capacity`, `validate_response_fields`, `encode_response`, `encode_response_headers`, および `capacity_tests` モジュール。

#### 二重バリデーションの維持と `is_valid_protocol_version` / `is_valid_version_for_encode` の関係

構築時バリデーション追加後も、`validate_response_fields` (`encoder.rs:363`) による encode 時バリデーションは**撤去せず維持する**。理由:

1. **防御の多重化**: デコーダー以外の経路 (将来の `serde` デシリアライズ等) でバリデーションを迂回した Response 構築が追加される可能性に備える
2. **異なる検証基準**: 構築時の `is_valid_protocol_version` は `token "/" DIGIT+ "." DIGIT+` (RTSP 拡張含む) を検証するが、encode 時は `is_valid_version_for_encode` (VCHAR のみ) で送信安全なバイトのみを保証する。両者は検証の目的が異なる (構文検証 vs 送信安全検証)。注: `is_valid_version_for_encode` は VCHAR 検査のみで RFC 9112 §2.3 の `HTTP-name = %s"HTTP"` (case-sensitive) を強制しておらず、`"http/1.1"` (小文字) も素通しになる。HTTP-name の case-sensitive 検証は本 issue のスコープ外とし、別 issue で対応する
3. **コストは微少**: `validate_response_fields` は O(n) (n = ヘッダー数) で、encode 全体のコストに対して無視できる

RFC 9112 / 9110 由来の意味的検証 (`Content-Length` と body の整合性、`Transfer-Encoding` と `Content-Length` の排他等) は当然維持する。

#### CONNECT 2xx の制約

RFC 9110 §6.4.1（CONNECT 2xx 応答は tunnel mode に切り替わり content を持たない）/ RFC 9112 §6.3（メッセージ長判定規則: client MUST ignore Content-Length or Transfer-Encoding）/ RFC 9110 §9.3.6 / RFC 9110 §8.6 により、CONNECT リクエストへの 2xx レスポンスには Transfer-Encoding / Content-Length を含めてはならない (MUST NOT)。本 issue でも encoder は **request method を知らない** ため、この制約は引き続き「呼び出し側の責務」として `encoder.rs:723-731` / `1005-1013` の現状コメントを維持する。コンストラクタ API レベルの強制は本 issue のスコープ外とし、必要なら別 issue で `for_connect_response()` ファクトリ等を検討する。

#### `validate_response_fields` のテスト到達性

新 API ではコンストラクタを通過した Response は `validate_response_fields` の各検証分岐を **必ず通過する** (構築時バリデーション ⊂ encode 時バリデーション、または同等)。これにより encoder 側のフィールド検証分岐は外部 API ではテスト到達不能になる。

対処: `encoder.rs` 内の `#[cfg(test)] mod tests` (既存 `capacity_tests` モジュール 1294 行付近に併設、または新規) で `Response::from_raw_parts` 経由で「検証未済の不正値」を持つ Response を構築し、`encode_response` がそれを `validate_response_fields` で弾くテストを 1 検証種別あたり 1 件以上追加する。`from_raw_parts` は `pub(crate)` なので同 crate のテストから呼び出し可能。

#### `Response::encode()` doc の更新

```rust
/// レスポンスをバイト列にエンコード
///
/// 構築時バリデーションで弾かれる構文上の不正値を含まない Response ならば、
/// 意味論的な RFC 違反（Content-Length 不一致等）がある場合にパニックする。
/// エラーハンドリングが必要な場合は `try_encode()` を使用する。
/// `from_raw_parts` 経由で構築された Response が release ビルドで構文エラーを
/// 含む場合もパニックする（encoder のエラーメッセージは構文/意味論を区別しない）。
pub fn encode(&self) -> Vec<u8> {
```

### src/lib.rs

doctest の `Response::new()` / `.header()` 呼び出しに `.unwrap()` を追加する:

```rust
let response = Response::new(200, "OK").unwrap()
    .header("Content-Type", "text/plain").unwrap()
    .body(b"Hello, World!".to_vec());
```

### tests / pbt / examples / fuzz

#### 書き換え概要

| 旧 API | 新 API |
|---|---|
| `Response::new(200, "OK")` | `Response::new(200, "OK").unwrap()` |
| `Response::with_version(v, s, r)` | `Response::with_version(v, s, r).unwrap()` |
| `Response { version, status_code, ... }` | `Response::with_version(v, s, r).unwrap()` + setter (外部)、`from_raw_parts` (crate 内) |
| `response.headers.push((n, v))` | `response.add_header(n, v).unwrap()` |
| `response.add_header(n, v)` | `response.add_header(n, v).unwrap()` |
| `.header(n, v)` | `.header(n, v).unwrap()` |
| `response.body = Some(data)` | `response = response.body(data)` |
| `response.body = None` | body None 復帰は別 issue (0021) で対応。本 issue では末尾 builder でのみ設定する |
| `response.omit_body = flag` | `response = response.omit_body(flag)` |
| `response.version` (参照) | `HttpHead::version(&response)` または必要なら `response` を `&impl HttpHead` で受ける |
| `response.status_code` (参照) | `response.status_code()` |
| `response.reason_phrase` (参照) | `response.reason_phrase()` |
| `response.headers.len()` / `response.headers[i]` (参照) | `HttpHead::headers(&response)` |
| `response.body.as_deref()` | `response.body()` (戻り値は `Option<&[u8]>`) |
| `response.omit_body` (参照) | `response.is_body_omitted()` |

`Response { ... }` 構造体リテラルの直接構築は全箇所で禁止される。crate 内は `from_raw_parts` を使用する。

#### pbt/tests/prop_response.rs の書き換え

既存の全 PBT はフィールド直接アクセス (`response.version`, `response.status_code`, `response.reason_phrase`, `response.headers`, `response.body`, `response.omit_body`) に依存している。すべてをアクセサ経由に書き換える:

- `&response.version` → `HttpHead::version(&response)`
- `response.status_code` → `response.status_code()`
- `&response.reason_phrase` → `response.reason_phrase()`
- `response.headers.len()` → `HttpHead::headers(&response).len()`
- `response.headers[i].0` / `.1` → `HttpHead::headers(&response)[i].0` / `.1`
- `response.body.is_none()` → `response.body().is_none()`
- `response.body.as_deref()` → `response.body()`
- `response.omit_body` → `response.is_body_omitted()`

#### 新規単体テスト (`tests/test_response.rs`)

```rust
// 不正 status_code
assert!(matches!(
    Response::new(0, "OK").unwrap_err(),
    EncodeError::InvalidStatusCode { code: 0 }
));
assert!(matches!(
    Response::new(600, "OK").unwrap_err(),
    EncodeError::InvalidStatusCode { code: 600 }
));

// 空 reason_phrase (RFC 1* 違反)
assert!(matches!(
    Response::new(200, "").unwrap_err(),
    EncodeError::InvalidReasonPhrase { .. }
));

// CRLF を含む reason_phrase
assert!(matches!(
    Response::new(200, "OK\r\nX-Inject: y").unwrap_err(),
    EncodeError::InvalidReasonPhrase { .. }
));

// NUL を含む reason_phrase
assert!(matches!(
    Response::new(200, "OK\0bad").unwrap_err(),
    EncodeError::InvalidReasonPhrase { .. }
));

// スペースを含むヘッダー名 (token 違反)
let mut r = Response::new(200, "OK").unwrap();
assert!(matches!(
    r.add_header("Bad Name", "x").unwrap_err(),
    EncodeError::InvalidHeaderName { .. }
));

// 空ヘッダー名
assert!(matches!(
    r.add_header("", "x").unwrap_err(),
    EncodeError::InvalidHeaderName { .. }
));

// CRLF を含むヘッダー値
assert!(matches!(
    r.add_header("X-Header", "value\r\n").unwrap_err(),
    EncodeError::InvalidHeaderValue { .. }
));

// LF のみのヘッダー値
assert!(matches!(
    r.add_header("X-Header", "value\n").unwrap_err(),
    EncodeError::InvalidHeaderValue { .. }
));

// NUL を含むヘッダー値 (RFC 9110 Section 5.5)
assert!(matches!(
    r.add_header("X-Header", "val\0ue").unwrap_err(),
    EncodeError::InvalidHeaderValue { .. }
));

// 空ヘッダー値は合法 (RFC 9110 Section 5.5: field-value = *field-content)
assert!(r.add_header("X-Empty", "").is_ok());

// 不正 version
assert!(matches!(
    Response::with_version("garbage", 200, "OK").unwrap_err(),
    EncodeError::InvalidVersion { .. }
));
assert!(matches!(
    Response::with_version("HTTP/1.1\r\nX: y", 200, "OK").unwrap_err(),
    EncodeError::InvalidVersion { .. }
));

// set_header の動作確認 (追加 + 上書き)
let mut r = Response::new(200, "OK").unwrap();
r.add_header("X-Custom", "first").unwrap();
r.set_header("X-Custom", "second").unwrap();
assert_eq!(r.get_headers("X-Custom").len(), 1);
assert_eq!(r.get_header("X-Custom"), Some("second"));

// set_header の case-insensitive 上書き
r.add_header("CONTENT-TYPE", "text/plain").unwrap();
r.set_header("Content-Type", "text/html").unwrap();
assert_eq!(r.get_header("Content-Type"), Some("text/html"));
```

#### 新規 PBT (`pbt/tests/prop_response.rs` に追加)

- 任意の妥当な `status_code` (proptest strategy: `100..=599u16`) と妥当な `reason_phrase` (VCHAR のみ、最低 1 文字) で `Response::new(code, phrase)` が `Ok` を返す
- 任意の妥当な `status_code` / `reason_phrase` / `version` (token `/` DIGIT+ `.` DIGIT+) で `Response::with_version(v, c, p)` が `Ok` を返す
- 戦略: 不正値を含む `reason_phrase` (制御文字 0x00-0x08, 0x0A-0x1F, 0x7F を含む文字列) で常に `Err(InvalidReasonPhrase)` が返る
- 戦略: 不正値を含むヘッダー名 (非 token 文字を含む文字列) で `add_header` / `header` が常に `Err(InvalidHeaderName)` を返す
- 戦略: 不正値を含むヘッダー値 (CR/LF/NUL/制御文字を含む文字列) で `add_header` / `header` が常に `Err(InvalidHeaderValue)` を返す
- 戦略: 不正な `version` で `with_version` が常に `Err(InvalidVersion)` を返す
- ラウンドトリップ: 妥当な入力で `Response::with_version(v, c, p)?` + `add_header(...)?` + `body(...)` で構築 → `encode_response` → `ResponseDecoder` でデコードした結果が、元の version / status_code / reason_phrase / headers / body と一致する (CLAUDE.md「PBT: 型情報 (Strategy) に基づいて入力を生成し、プロパティを検証する (ラウンドトリップ等)」に沿う構築 ↔ encode ↔ decode の三者一致)
- ラウンドトリップ（reason-phrase absent）: `from_raw_parts` を使って空 `reason_phrase` + 妥当なその他フィールドで構築 → `encode_response` → `ResponseDecoder` でデコードした結果が `HttpHead::headers` / `body` を保存しつつ `reason_phrase()` が空文字列を返すこと（reverse proxy 経路の最重要ケース）
- アクセサのラウンドトリップ: 任意の妥当な `status_code`, `reason_phrase`, `body` で `Response::new()` → `.status_code() == code` かつ `.reason_phrase() == phrase` かつ `.body() == body`、body 未設定時は `.body() == None`、`.is_body_omitted() == false`（デフォルト値検証）

#### fuzz ターゲットの改修

`fuzz/fuzz_targets/fuzz_encode_response.rs`:

変更前:
```rust
let mut response = Response::with_version(&version, status_code, &reason_phrase);
for (name, value) in &headers {
    response.add_header(name, value);
}
response.body = if body_present { Some(body) } else { None };
response.omit_body = omit_body;
```

変更後:
```rust
let Ok(mut response) = Response::with_version(&version, status_code, &reason_phrase)
else { return; };
for (name, value) in &headers {
    if response.add_header(name, value).is_err() { return; }
}
// body_present に基づいて body を設定 or しない (body = None のパスを残す)
let response = if body_present {
    response.body(body)
} else {
    response
};
let response = response.omit_body(omit_body);
```

注: `body = None` のケースを残すため、条件分岐で `body_present: false` のときは `body()` を呼び出さない。これにより fuzz は `body = None` / `body = Some(...)` の両パスをカバーし続ける。

`fuzz/fuzz_targets/fuzz_decoder_roundtrip.rs`:
- `response.body = Some(...)` → `response = response.body(...)` に書き換え
- ただし `has_body && !body.is_empty()` の条件に合わせ、body 設定は条件付きで行う

`fuzz/fuzz_targets/fuzz_request_response_helpers.rs`:
- `response.body = Some(body)` → `response = response.body(body)` に書き換え

### examples の改修方針

CLAUDE.md「サンプルは **お手本** なので性能と堅牢性を両立させる」原則に従い、examples は **`.unwrap()` を使わず `?` 伝播する** 設計に書き換える。

- 静的リテラルのヘッダー (`"Server"`, `"Content-Type"`, `"Date"` 等) は `.expect("static header is valid")` を許容するが、メッセージで「リテラルが妥当だから expect」を明示する
- 動的入力 (upstream のヘッダー値、`final_body.len().to_string()` 等) には必ず `?` を使う
- `build_response` / `build_compressed_response` 等の戻り値を `Result<Response, EncodeError>` に変更し、呼び出し元 (`serve_request` 等) は `?` で伝播する。`EncodeError: std::error::Error` のため `Box<dyn std::error::Error + Send + Sync>` への `From` 自動変換が効く

### Request 側

`Request` も同様の問題を抱えているが、本 issue は `Response` 限定とする。`Request` 側は別 issue で対応する。

## CHANGES.md

`## develop` セクションに以下を追加する:

```
- [CHANGE] `Response` の全フィールドを非公開化し、構築時バリデーションを追加する
  - 構築は `Response::new` / `Response::with_version` が `Result<Self, EncodeError>` を返す形に変更する
  - `add_header` / `header` でヘッダー名 (RFC 9110 Section 5.1 token) と値 (RFC 9110 Section 5.5 field-value, CR/LF/NUL 不可) をバリデートし `Result` を返す
  - `set_header` を新設し、同名ヘッダーの上書きを可能にする
  - `pub(crate) fn from_raw_parts` を新設し、デコーダー内部からの検証済み構築を可能にする
  - `status_code()` / `reason_phrase()` / `version()` / `body()` / `is_body_omitted()` の読み取り専用アクセサを追加する
  - 構造体に `#[non_exhaustive]` を付与する
  - `Response::encode()` のパニック条件を意味論的違反 (Content-Length 不一致等) に限定した doc に更新する
  - `encoder.rs` 内部の全フィールド直接アクセスをアクセサ経由に書き換える
  - @voluntas
```

注: `is_valid_reason_phrase` の空文字列拒否追加は `pub(crate)` の内部変更であり、呼び出し側が空文字列をスキップするため外部 API の観測可能な挙動に変化はない。CHANGES.md では `### misc` の `[UPDATE]` として扱う。

```
### misc

- [UPDATE] (crate 内部) `is_valid_reason_phrase` を RFC 9112 §4 ABNF `1*(HTAB / SP / VCHAR / obs-text)` に厳密準拠させ、空文字列を非合法と判定する。decoder / encoder の呼び出し側で reason-phrase absent (空文字列) はスキップする方式に統一する
  - @voluntas
```

## 検証方針

### 不変条件が構築時点で守られることの確認

新規単体テスト (`tests/test_response.rs`) で以下を検証する:

- 不正な `status_code` (0, 600) で `Err(InvalidStatusCode)` が返る
- 空文字列の `reason_phrase` で `Err(InvalidReasonPhrase)` が返る
- CRLF を含む `reason_phrase` で `Err(InvalidReasonPhrase)` が返る
- NUL を含む `reason_phrase` で `Err(InvalidReasonPhrase)` が返る
- スペースを含むヘッダー名で `Err(InvalidHeaderName)` が返る
- 空文字列のヘッダー名で `Err(InvalidHeaderName)` が返る
- CRLF を含むヘッダー値で `Err(InvalidHeaderValue)` が返る
- LF のみを含むヘッダー値で `Err(InvalidHeaderValue)` が返る
- NUL を含むヘッダー値で `Err(InvalidHeaderValue)` が返る
- 空ヘッダー値が合法であること
- token/DIGIT.DIGIT 形式に違反する version で `Err(InvalidVersion)` が返る
- `set_header` が case-insensitive に上書きできる

### reason-phrase absent のデコーダー互換性確認

- decoder が `HTTP/1.1 200 \r\n` (reason-phrase absent) を正しく受理できること
- 修正後の `is_valid_reason_phrase` が空文字列で `false` を返すが、decoder 側では空文字列をスキップすることで正常動作することを確認

### 既存挙動が回帰しないことの確認

- 既存の単体テスト (`tests/test_encoder.rs` 等) が新 API に追従して green になる
- PBT (`prop_response.rs`, `prop_encoder.rs`, `prop_decoder/response.rs` 等) が新 API に追従して green になる
- fuzz ターゲット (`fuzz_encode_response`, `fuzz_decoder_roundtrip`, `fuzz_request_response_helpers`, `fuzz_decoder_chunked`) が新 API に追従して green になる
- 全 examples (`http11_server`, `http11_server_io_uring`, `http11_reverse_proxy`, `http11_client`) がコンパイルおよび実行可能である

### カバレッジ検証

```bash
cargo llvm-cov clean --workspace
cargo llvm-cov --no-report -p shiguredo_http11 --lib -- response validate
cargo llvm-cov --no-report -p shiguredo_http11 --lib -- encoder
cargo llvm-cov --no-report -p shiguredo_http11 --test test_response
cargo llvm-cov --no-report -p pbt --test prop_response
cargo llvm-cov report
```

`Response::new` / `with_version` / `add_header` / `header` / `set_header` / `from_raw_parts` の全バリデーション分岐 (成功パス・失敗パス) がカバーされていることを確認する。`is_valid_reason_phrase` の空文字列拒否パス + decoder 側の absent スキップパスの両方がカバーされていることを確認する。

## 受け入れ基準

- ブランチ名が `feature/change-response-fields-private-with-validation` であること
- `make fmt && make clippy && make check && make test` がすべて成功する
- `src/response.rs` から `pub version` / `pub status_code` / `pub reason_phrase` / `pub headers` / `pub body` / `pub omit_body` が消えている
- `Response` 構造体に `#[non_exhaustive]` が付いている
- `pub(crate) fn from_raw_parts` が Response に存在し、decoder がこれを使用している
- バリデーションエラー再現テストが全種成功する
  - `InvalidStatusCode` (0, 600)
  - `InvalidReasonPhrase` (空文字列, CRLF 含む, NUL 含む)
  - `InvalidHeaderName` (スペース含む, 空文字列)
  - `InvalidHeaderValue` (CRLF 含む, LF 含む, NUL 含む)
  - `InvalidVersion` (不正形式)
- 空ヘッダー値が合法であることのテストが存在する
- `set_header` の上書き動作が検証されている
- `set_header` の doc に「Set-Cookie のように同名複数値が意味を持つヘッダーには使ってはならない」という警告が含まれている（RFC 6265 参照）
- `is_valid_reason_phrase` が空文字列を拒否する (`!phrase.is_empty()` が追加されている)
- decoder が reason-phrase absent (`HTTP/1.1 200 \r\n`) を受理できる
- `encoder.rs` 内部で `response.version` / `response.status_code` / `response.reason_phrase` / `response.headers` / `response.body` / `response.omit_body` への直接フィールドアクセスが残っていない
- `cargo llvm-cov` でコンストラクタ / `add_header` / `set_header` / `from_raw_parts` のバリデーション分岐がカバーされている
- 全 fuzz ターゲットが新 API に追従しコンパイル可能である
- `set_header` のバリデーション失敗時に既存ヘッダーが消えない (アトミック性) ことが単体テストで検証されている
- 全 examples が `.unwrap()` を使わず `?` 伝播で書かれている (静的リテラルへの `.expect("...")` は理由付きで許容)
- `encoder.rs` 内の `validate_response_fields` 各検証分岐が `from_raw_parts` 経由のテストでカバーされている (構築時バリデーションでは到達不能になる分岐の coverage 補填)
- `skills/shiguredo-http11/SKILL.md` の Response API 一覧およびサンプルコードが新 API に追従している
- `pbt/tests/proptest-regressions/response.txt` の既存シードが新 API でも再現可能であること (確認の上で更新不要)
- decoder が空 reason-phrase (`HTTP/1.1 200 \r\n`) を `from_raw_parts` 経由で受理し、それを encoder で再エンコードした結果が `HTTP/1.1 200 \r\n` (status-code の後に SP 1 個 + CRLF) になることを単体テストまたは PBT で確認する (reverse proxy 経路の RFC 9112 §4 準拠維持)
- `from_raw_parts` の不変条件が `debug_assert!` で検査されており、debug ビルドで契約破りを検出可能であること

## 解決方法

- `src/response.rs` の全フィールドを非公開化し `#[non_exhaustive]` を付与
- `Response::new` / `with_version` を `Result<Self, EncodeError>` 化し構築時バリデーション (status_code / reason_phrase / version) を実装
- `add_header` / `header` を `Result` 化し、ヘッダー名 (RFC 9110 token) と値 (CR/LF/NUL 拒否) のバリデーションを実装
- `set_header` を新設し、case-insensitive 上書きを提供 (バリデーション失敗時のアトミック性確保)
- `pub(crate) fn from_raw_parts` を新設しデコーダー経路の検証済み構築を可能にし、`debug_assert!` で契約を表明
- 読み取り専用アクセサ `status_code()` / `reason_phrase()` / `version()` / `body_bytes()` / `is_body_omitted()` を追加
  - 注: builder `body(data) -> Self` と getter `body() -> Option<&[u8]>` の同名併存は Rust の inherent impl 制約により不可能なため、getter は `body_bytes` に改名 (issue 本文の「同名併存可能」記述は Rust の仕様誤認だった)
- `validate.rs` の `is_valid_reason_phrase` を RFC 9112 §4 ABNF `1*(...)` に厳密準拠させ空文字列を拒否
- decoder / encoder の `is_valid_reason_phrase` 呼び出しは空文字列 (reason-phrase absent) をスキップする形に統一
- `encoder.rs` 内部の全フィールド直接アクセスをアクセサ経由 (`HttpHead` トレイト経由含む) に書き換え
- `Response::encode()` の doc を意味論的違反 (Content-Length 不一致等) に限定する旨に更新
- 全 examples (`http11_server`, `http11_server_io_uring`, `http11_reverse_proxy`, `http11_client`) を `?` 伝播形式に書き換え
- 全 tests / PBT / fuzz ターゲットを新 API に追従、`tests/test_response.rs` を新設して構築時バリデーションのエラーパスを網羅
- `SKILL.md` の Response API 一覧およびサンプルコードを新 API に追従
- `make fmt && make clippy && make check && make test` がすべて成功することを確認
