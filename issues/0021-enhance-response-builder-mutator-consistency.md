# 0021: Response のビルダーと mutator API を整備する

Created: 2026-05-06
Model: Opus 4.7 / DeepSeek V4 Pro

## 概要

`Response` のビルダー API と mutator API の欠落・非対称・冗長アロケーションを解消する。具体的には:

- mutator (`add_header`, `set_header`) の戻り値を `Result<&mut Self, EncodeError>` にしてチェイン可能にする ([CHANGE])
- ビルダー / mutator の対応が片寄っているフィールド (body, omit_body) について、両系統を対称に提供する ([ADD])
- 文字列・バイト列受け取りを `impl Into<String>` / `impl Into<Vec<u8>>` に変更し、呼び出し側が所有値をムーブ可能にする ([UPDATE])
- `set_header` の引数も同様に `impl Into<String>` 化する ([UPDATE])

ブランチ名: `feature/change-response-builder-mutator-consistency`。破壊的変更 (`add_header` / `set_header` の戻り値型変更) を含むため `feature/change-` を使用する。

依存関係: `0017` (フィールド非公開化とバリデーション付き構築) は完了済み。`0017` で `Response` のフィールドが非公開化され、`new` / `with_version` / `header` / `add_header` / `set_header` が `Result` を返すようになっている。`body_bytes()` getter が新設され、builder `body(data)` との同名衝突は `body_bytes` 命名で解決済み。本 issue はその上に、戻り値型の変更 (`&mut Self` 返却) と引数型の変更 (`impl Into`) を重ねる。また getter `body_bytes()` は維持し、改名しない (0017 の設計判断を踏襲)。

`0024` (StatusCode 型導入) は完了済み。`Response::with_status` が利用可能である。

## 根拠

### 問題 1: ビルダーと mutator の提供範囲が非対称

| 操作 | ビルダー (Self → Self) | mutator (&mut Self) |
|---|---|---|
| ヘッダー追加 | `header(self, name, value)` | `add_header(&mut self, name, value)` |
| ボディ設定 | `body(self, body)` | (なし) |
| ボディ削除 | (なし) | (なし) |
| omit_body 設定 | `omit_body(self, omit)` | (なし) |

`header` だけ両系統あり、`body` / `omit_body` はビルダーのみ。さらにボディ削除 (`body = None`) は両系統とも欠けている。設計の判断基準が一貫していない。

### 問題 2: mutator がチェインできない

```rust
// 0017 完了後の現行シグネチャ
pub fn add_header(&mut self, name: &str, value: &str) -> Result<(), EncodeError>
```

戻り値が `Result<(), EncodeError>` のため、`response.add_header(a, b)?.add_header(c, d)?` のチェイン呼び出しができない（`()` には `add_header` が実装されていない）。`Result<&mut Self, EncodeError>` を返せば `?` 伝播で連続呼び出しが書ける。

### 問題 3: 引数の重複アロケーション

```rust
// 0017 完了後の現行シグネチャ (src/response.rs:246)
pub fn add_header(&mut self, name: &str, value: &str) -> Result<(), EncodeError> {
    // ...
    self.headers.push((name.to_string(), value.to_string()));
    // ...
}
```

`&str` を受けて即 `to_string()` するため、呼び出し側が既に `String` を持っていてもムーブできず必ずクローンする。`impl Into<String>` で受ければ `String` ・ `&str` 両方に対応でき、`String` の場合はムーブで済む。

対象メソッド:
- `Response::new` (status_code: u16 は変更なし、reason_phrase のみ `impl Into<String>` 化)
- `Response::with_version` (version, reason_phrase)
- `Response::header` (name, value)
- `Response::add_header` (name, value)
- `Response::body` (body: `Vec<u8>` → `impl Into<Vec<u8>>`)
- `Response::set_body` (同上)
- `Response::set_header` (name, value — `0017` で新設されたメソッド。本 issue では戻り値型も `&mut Self` 返却に変更し、引数も `impl Into<String>` 化する)

HTTP ヘッダー名・値は高々数十バイトの文字列であるため、この変更は致命的なボトルネックの解消ではないが、所有値をムーブ可能にすることで不必要なクローンを避けられる。`impl Into<String>` は Rust の標準的なパターンであり、API の表現力を損なわず、既存の `&str` 呼び出しも引き続きコンパイル可能である。

### 問題 4: `header` と `add_header` の命名が非対称

| 方向 | メソッド名 |
|---|---|
| ビルダー (`self`) | `header` |
| mutator (`&mut self`) | `add_header` |

どちらも `headers.push(...)` の動作であり、命名が非対称。ただし body 側では `body` (builder) / `set_body` (mutator) と `set_` プレフィックスで区別しており、header 側も builder は `header`、mutator は `add_header` で実質的な動作 (追加) を表現している。本 issue ではこの非対称性を認識した上で、破壊的変更を避けるため既存命名を維持する。body / omit_body の新規 mutator には `set_` プレフィックスを採用し、今後の命名規則を統一する。

### 問題 5: 0017 の `body` ゲッター / ビルダー名前衝突 (解決済み)

0017 は `pub fn body(&self) -> Option<&[u8]>` (getter) と `pub fn body(mut self, body: Vec<u8>) -> Self` (builder) の両方の提案を検討したが、実装時に inherent impl の制約で同名併存が不可能であることが判明し、getter は `body_bytes()` として解決済み (`src/response.rs:309`)。

本 issue ではこの命名を踏襲する。`body_bytes()` → `as_body()` への再改名は行わない (二度目の破壊的改名を避ける。`as_` プレフィックスによる命名統一は別 issue で検討)。本 issue のコード例・テストはすべて `body_bytes()` を使用する。

### RFC 準拠の根拠

本 issue で変更する API 要素の根拠となる RFC 要件:

| API 要素 | 引用 RFC | 要件 |
|---|---|---|
| ヘッダー名形式 | RFC 9110 §5.1 | `field-name = token` 。1 文字以上の tchar 集合に制限される。**Field names are case-insensitive** (大文字小文字を区別しない) |
| ヘッダー値形式 | RFC 9110 §5.5 | `field-value = *field-content` であり空値は合法。field-value に CR, LF, NUL を含めてはならない。CTL 文字は invalid だが recipients MAY retain (安全な文脈に限る)。`field-content` は `field-vchar [ 1*( SP / HTAB / field-vchar ) field-vchar ]` で、VCHAR または obs-text で開始・終了しなければならない (構造制約) |
| ヘッダー名重複 | RFC 9110 §5.3 (Field Order) | 同一フィールド名の複数行生成は、そのフィールド定義が許容する場合を除き禁止 (MUST NOT)。Set-Cookie は実際にはこの要件に違反する形で複数行使用されることがある (RFC 9110 §5.3 注記) |
| reason-phrase 形式 | RFC 9112 §4 | `reason-phrase = 1*( HTAB / SP / VCHAR / obs-text )` 。少なくとも 1 文字必要 |
| reason-phrase 省略 | RFC 9110 §15.1 | reason-phrase は推奨値のみで、置換または省略可能 |
| HTTP-version 形式 | RFC 9112 §2.3 | `HTTP-version = HTTP-name "/" DIGIT "." DIGIT` (status-line 経由、ABNF 定義は §2.3) |
| status-code 形式 | RFC 9112 §4 | `status-code = 3DIGIT` |
| body なしレスポンス | RFC 9110 §6.4.1 / RFC 9110 §9.3.2 / RFC 9112 §6.1 | 1xx / 204 / 304 レスポンスは content を含まない (§6.4.1)。HEAD レスポンスも content を含まない (§9.3.2)。CONNECT 2xx は content を含まない (RFC 9112 §6.1) |
| body 長決定 | RFC 9112 §6.3 | HEAD / 1xx / 204 / 304 は空行で終端し、message body も **trailer section** も存在しない |
| Content-Length 禁止 | RFC 9110 §8.6 | 1xx / 204 / CONNECT 2xx レスポンスでは Content-Length を送信してはならない (MUST NOT)。304 には明示的に禁止されていないが body 自体が存在しない (§6.3) |
| Content-Length in HEAD | RFC 9110 §8.6 | HEAD レスポンスで Content-Length を送信してよい (MAY) が、GET の場合に送信されたであろう値と等しい場合に限る (MUST NOT otherwise) |

注: ヘッダー名・値のバリデーションは `0017` で `is_valid_header_name` / `is_valid_field_value` を用いて実装済み。本 issue はバリデーション内容を変更しない。

## 対応方針

### 影響範囲一覧

| ファイル | 種別 | 内容 |
|---|---|---|
| `src/response.rs` | 主要変更 | `add_header` 戻り値変更、body/omit_body mutator 追加、`impl Into` 化、`without_body` 追加、`set_header` の戻り値型/引数型変更 |
| `src/encoder.rs` | **変更不要** | `body_bytes()` 参照箇所は改名不要のため変更なし。`add_header` 戻り値変更の影響なし (encoder は Response を消費しない) |
| `pbt/tests/prop_response.rs` | 修正 | 新規 mutator PBT 追加 (`set_body`, `clear_body`, `without_body`, `set_omit_body`, チェインテスト) |
| `pbt/tests/prop_encoder.rs` | **変更不要** | `Response` ビルダー呼び出しは `impl Into` 化後も `&str` / `Vec<u8>` 互換のため |
| `pbt/tests/prop_decoder/response.rs` | **変更不要** | `body_bytes()` への参照が存在するが改名不要のため |
| `pbt/tests/prop_decoder/body.rs` | **変更不要** | 同上 |
| `fuzz/fuzz_targets/fuzz_encode_response.rs` | 修正 | `body_present: false` パスで `clear_body()` を呼ぶ形に変更（body=None を明示的にカバーするため）。`omit_body` 行を `set_omit_body` に移行 |
| `fuzz/fuzz_targets/fuzz_request_response_helpers.rs` | **変更不要** | `body = None` は `Response::new` の初期状態で既にカバー済み。`body_present: bool` 追加は責務外のため不要 |
| `fuzz/fuzz_targets/fuzz_decoder_roundtrip.rs` | **変更不要** | `body = None` は 1xx/204/304 で `Response::new` の初期状態により既にカバー済み |
| `fuzz/fuzz_targets/fuzz_decoder_chunked.rs` | **変更不要** | `add_header` 戻り値変更後も `.unwrap()` で正常動作。変更不要 |
| `examples/http11_reverse_proxy/src/main.rs` | **変更不要** | 全 `add_header` 呼び出しは既に `?` 演算子で Result を消費済み。戻り値型が `Result<&mut Self, E>` になっても正常動作 |
| `examples/http11_server/src/main.rs` | 修正 | `Response` 構築箇所の追従確認 |
| `examples/http11_server_io_uring/src/main.rs` | 修正 | 同上 |
| `examples/http11_client/src/main.rs` | 修正 | 同上 |
| `tests/test_encoder.rs` | 修正 | `Response` ビルダー呼び出しの追従確認 |
| `tests/test_response.rs` | 修正 | `clear_body` / `without_body` / チェイン動作 / `set_omit_body` の単体テストを追記 (0017 で作成済みのファイルに追記) |
| `tests/test_decoder.rs` | **変更不要** | `body_bytes()` 参照は改名不要のため |
| `tests/test_decode_body.rs` | **変更不要** | 同上 |
| `CHANGES.md` | 修正 | `## develop` にエントリ追加 |

### src/response.rs

#### 1. `add_header` のチェイン化

```rust
/// ヘッダーを追加
///
/// 名前は RFC 9110 §5.1 の token (1*tchar)、
/// 値は RFC 9110 §5.5 の field-value を満たす必要がある。
/// field-value は CR, LF, NUL を含まず、field-vchar (VCHAR または obs-text)
/// で開始・終了しなければならない (field-content 構造制約)。
/// バリデーション成功後にヘッダーが追加される (失敗時は self に変更なし)。
pub fn add_header(
    &mut self,
    name: impl Into<String>,
    value: impl Into<String>,
) -> Result<&mut Self, EncodeError> {
    // バリデーション → 成功時のみ push (順序保証)。
    // 注: .into() はバリデーション前に実行されるため、無効な入力でも
    // アロケーションが発生する。これは impl Into で所有値のムーブを
    // 受け付けるためのトレードオフであり、0017 の add_header 実装における
    // 「バリデーション → to_string → push」の順序と同じである。
    let name = name.into();
    let value = value.into();
    if !is_valid_header_name(&name) {
        return Err(EncodeError::InvalidHeaderName { name });
    }
    if !is_valid_field_value(&value) {
        return Err(EncodeError::InvalidHeaderValue {
            name,
            value,
        });
    }
    self.headers.push((name, value));
    Ok(self)
}
```

注: バリデーションとミューテーションの順序は「バリデーション成功後にのみ push」を保証する。`add_header(a, b)?` が成功した後、後続の `add_header(c, d)?` が失敗した場合、a, b のヘッダーは追加済みのままになる。`Result<&mut Self, E>` のチェインとしては自然な振る舞いであり、Rust の `?` 伝播の一般的な動作と一致する。

#### 2. body / omit_body の mutator 追加

```rust
/// ボディを設定 (mutator)
pub fn set_body(&mut self, body: impl Into<Vec<u8>>) -> &mut Self {
    self.body = Some(body.into());
    self
}

/// ボディを削除 (mutator)
///
/// body を None に設定する。明示的に空ボディ (Content-Length: 0) を
/// 設定したい場合は `set_body(Vec::new())` を使用すること。
pub fn clear_body(&mut self) -> &mut Self {
    self.body = None;
    self
}

/// ボディ送信抑止フラグを設定 (mutator)
///
/// HEAD レスポンスではヘッダーのみ送信し、メッセージボディを送信しない
/// (RFC 9110 Section 9.3.2 / RFC 9110 Section 6.4.1)。
/// Content-Length は表現長として残しつつメッセージボディを送信しない場合に使用する
/// (Content-Length を HEAD レスポンスで送信できる根拠は RFC 9110 Section 8.6:
/// MUST NOT send Content-Length unless its field value equals the decimal number
/// of octets that would have been sent in the content if the same request had used GET)。
pub fn set_omit_body(&mut self, omit: bool) -> &mut Self {
    self.omit_body = omit;
    self
}
```

`body = None` (ボディなし、Content-Length 自動付与なし) と `body = Some(vec![])` (明示的空ボディ、Content-Length: 0 自動付与) の区別:
- `set_body(Vec::new())` → `body = Some(vec![])` (空ボディ)
- `clear_body()` / `without_body()` → `body = None` (ボディ意図なし)

この区別は `closed/0004-change-request-response-body-optional.md` の設計判断に基づき、型レベルで表現済み。

`clear_body` の命名について: `Vec::clear()` は「内容を空にする」セマンティクスだが、`clear_body` は「body を None にする = body を持たない状態に戻す」操作である。`remove_body` や `unset_body` よりも短く、`clear` の「片付ける/消す」という日常語感覚と `body = None` の初期化操作としての一致を優先した。空ボディ (`Some(vec![])`) との混同が懸念される場合は `remove_body` への改名を別途検討する。

#### 3. body ビルダーの `without_body` 追加

```rust
/// ボディなしを明示 (ビルダーパターン)
///
/// `body = None` に設定する。builder チェイン中に `body()` を呼んだ後で
/// ボディを取り消す場合に使用する。HEAD レスポンスを生成するユースケースでは
/// 最初から `body()` を呼ばなければよいが、条件分岐で builder chain を構築する
/// ユーティリティコードでの利用を想定する。
pub fn without_body(mut self) -> Self {
    self.body = None;
    self
}
```

`no_body` ではなく `without_body` を採用する理由: `no_body` は `omit_body` (ボディ送信抑止) と区別がつきにくい。`without_body` は「ボディを持たない」を明確に表現し、`omit_body` との混同を避ける。`without_body()` と `omit_body(true)` は全く異なる操作であり、命名の類似性が誤用を招く可能性があるため、`without_body` の doc に `omit_body` との違いを明記する。

#### 4. 引数の `impl Into<String>` / `impl Into<Vec<u8>>` 化

```rust
pub fn new(status_code: u16, reason_phrase: impl Into<String>) -> Result<Self, EncodeError>
pub fn with_version(version: impl Into<String>, status_code: u16, reason_phrase: impl Into<String>) -> Result<Self, EncodeError>
pub fn header(self, name: impl Into<String>, value: impl Into<String>) -> Result<Self, EncodeError>
pub fn add_header(&mut self, name: impl Into<String>, value: impl Into<String>) -> Result<&mut Self, EncodeError>
pub fn body(mut self, body: impl Into<Vec<u8>>) -> Self   // Result 不要: バリデーションなし
pub fn set_body(&mut self, body: impl Into<Vec<u8>>) -> &mut Self
```

注: `body` / `set_body` / `clear_body` / `without_body` / `omit_body` / `set_omit_body` はバリデーションを必要としないため `Result` を返さず、`Self` または `&mut Self` を直接返す。body mutation に関する RFC 上の制約 (1xx/204/304 は body 不可 等) は encoder 側で検証済みであり、mutator 側での再検証は行わない (`0017` の「encoder による二重バリデーション維持」方針と一致)。

注: `impl Into<Vec<u8>>` は `Vec<u8>` (ムーブ) と `&[u8]` (clone) の両方で動作する。`&[u8]` → `Vec<u8>` の変換は標準ライブラリの `From<&[T]> for Vec<T>` 経由でクローンが発生するため、`&[u8]` 呼び出し側でのクローン回避効果はない。`Vec<u8>` を所有する呼び出し側が `body(existing_vec)` のようにムーブで渡せるようになることが本変更の主目的。既存の呼び出し側はすべて `.body(b"hello".to_vec())` または `.body(vec![...])` の形式であり、`impl Into<Vec<u8>>` 化後もコンパイル可能。

#### 5. `set_header` の `impl Into<String>` 化と `&mut Self` 返却

`0017` で新設された `set_header` も同様に `impl Into<String>` 化し、戻り値も `&mut Self` 返却に変更する:

```rust
/// 指定した名前の既存ヘッダーを全削除し、新規に追加する
///
/// 同名 (case-insensitive) のヘッダーをすべて削除した後、
/// 指定した name/value で新規追加する。
/// バリデーション失敗時は既存ヘッダーは変更されない (アトミック性の保証)。
pub fn set_header(
    &mut self,
    name: impl Into<String>,
    value: impl Into<String>,
) -> Result<&mut Self, EncodeError> {
    let name = name.into();    // アロケーションはバリデーション前に発生する
    let value = value.into();  // (impl Into で所有値を受けるためのトレードオフ)
    if !is_valid_header_name(&name) {
        return Err(EncodeError::InvalidHeaderName { name });
    }
    if !is_valid_field_value(&value) {
        return Err(EncodeError::InvalidHeaderValue { name, value });
    }
    // retain は &name (Deref<Target=str>) で借用し、push で (name, value) を move する
    self.headers.retain(|(n, _)| !n.eq_ignore_ascii_case(&name));
    self.headers.push((name, value));
    Ok(self)
}
```

注: `set_header` は本 issue で `&mut Self` を返すように変更する (`0017` では `Result<(), EncodeError>` で定義された)。ヘッダー操作系の mutator はすべて `Result<&mut Self, EncodeError>` を返すことでチェイン可能にする。`impl Into<String>` 化に伴い、バリデーション前に `.into()` が実行されるため無効な入力でもアロケーションが発生するが、これは所有値のムーブを受け付けるためのトレードオフであり、0017 の `set_header` 実装における「アトミック性のためバリデーションを先に行う」という設計意図からの逸脱であることを認識した上で採用する。

#### 6. `#[must_use]` と既存コードの対応

`Result<T, E>` には `#[must_use]` が付与されているが、`add_header` / `set_header` の戻り値型を `Result<(), E>` から `Result<&mut Self, E>` に変更しても `#[must_use]` の有無は変わらない。

0017 完了後の現行コードでは、全 `add_header` 呼び出しが既に `.is_err()` / `?` / `.unwrap()` で Result を消費済みであるため、`unused_must_use` 警告は発生しない。以下の確認で十分:

| 影響ファイル | 現行の Result 処理 | 本 issue での対応 |
|---|---|---|
| `fuzz/fuzz_targets/fuzz_encode_response.rs` | `.is_err() { return; }` | 変更不要 (`.is_err()` が Result を消費、`&mut Self` は drop され次の borrow が可能) |
| `fuzz/fuzz_targets/fuzz_request_response_helpers.rs` | `.is_err() { return; }` | 同上 |
| `fuzz/fuzz_targets/fuzz_decoder_roundtrip.rs` | `.is_err() { return; }` | 同上 |
| `fuzz/fuzz_targets/fuzz_decoder_chunked.rs` | `.unwrap()` | 同上 (`.unwrap()` で消費) |
| `examples/http11_reverse_proxy/src/main.rs` | `?` | 同上 (`?` で消費) |

注: `add_header` が `Result<&mut Self, E>` を返すようになった後も、`.is_err()` は `Result` を消費し `&mut Self` を drop するため、次の行で `response` を再 borrow できる。既存の fuzz コードは変更不要。

#### 0017 / 0018 との依存関係と命名の取り扱い

`pending/0018` (omit_body 撤去提案) の成否によって `set_omit_body` の有無が変わる。本 issue では 0018 が pending のままでも実装可能なように、以下の分岐方針を取る:

- `0018` が **reject された場合**: `set_omit_body` をそのまま残す
- `0018` が **accept され `omit_body` が撤去された場合**: `set_omit_body` を省く。`without_body` / `clear_body` は `omit_body` 撤去後も意味を持つ (`body = None` 設定) ため残す

`0017` は完了済みで getter 名は以下に確定している:
- body getter: `body_bytes()` (builder `body(data)` との同名衝突回避のため)
- omit_body getter: `is_body_omitted()`

本 issue の mutator 命名はこの命名を前提とする:
- `set_body()` / `clear_body()` → getter `body_bytes()` とのペア (非対称だが許容)
- `set_omit_body()` → getter `is_body_omitted()` との setter/getter ペア

### tests / pbt / fuzz / examples

#### PBT (`pbt/tests/prop_response.rs`)

`0017` により全テストが Result 処理の追加を必要としている (完了済み)。本 issue では新規 API の PBT を追加する:

既存テストの修正方針:
- `Response::new(code, "OK")` → `Response::new(code, "OK").unwrap()` (0017 で対応済み)
- `.header("Content-Type", "text/html")` → `.header("Content-Type", "text/html").unwrap()` (0017 で対応済み)
- フィールド直接アクセス (`response.headers.len()`) → getter 経由 (`HttpHead::headers(&response).len()` — 0017 で対応済み)
- `.body(data.clone())` → 変更不要 (`impl Into<Vec<u8>>` は `Vec<u8>` をそのまま受け付ける)
- `body_bytes()` → 改名不要 (維持)

新規追加 PBT:
- `set_body` → `body_bytes()` (getter) のラウンドトリップ
- `set_body` → `clear_body` → `body_bytes()` が `None` になること
- `without_body` ビルダー → `body_bytes()` が `None` になること
- `add_header` チェイン: `response.add_header(a, v)?.add_header(b, w)?` で両方のヘッダーが追加されること
- `add_header` → `clear_body` → `set_body` の mutator チェイン
- `impl Into<String>` に `&str` と `String` の両方を渡せること (Strategy で両方生成 — `String` は `any::<String>()` でよく、`&str` は strategy から生成した `String` を `.as_str()` で借用すればライフタイム問題を回避できる)
- `impl Into<Vec<u8>>` に `Vec<u8>` を渡せること

#### 単体テスト (`tests/test_response.rs`)

`0017` で作成済みのファイルに以下を追記する。テスト関数は `Result<(), EncodeError>` または `Result<(), Box<dyn Error>>` を返すシグネチャが必要 (`?` 演算子使用のため):

```rust
// clear_body で body が None になる
let mut r = Response::new(200, "OK").unwrap();
r.set_body(b"data".to_vec());
assert!(r.body_bytes().is_some());
r.clear_body();
assert!(r.body_bytes().is_none());

// without_body ビルダーで body が None になる
let r = Response::new(200, "OK").unwrap()
    .body(b"data".to_vec())
    .without_body();
assert!(r.body_bytes().is_none());

// set_body で body が設定される
let mut r = Response::new(200, "OK").unwrap();
r.set_body(b"hello".to_vec());
assert_eq!(r.body_bytes(), Some(b"hello".as_slice()));

// set_omit_body で omit_body が設定される
let mut r = Response::new(200, "OK").unwrap();
r.set_omit_body(true);
assert!(r.is_body_omitted());
r.set_omit_body(false);
assert!(!r.is_body_omitted());

// add_header チェイン呼び出し
let mut r = Response::new(200, "OK").unwrap();
r.add_header("X-A", "1")?.add_header("X-B", "2")?;
// get_headers は Vec<&str> を返す
assert_eq!(r.get_headers("X-A"), vec!["1"]);
assert_eq!(r.get_headers("X-B"), vec!["2"]);

// add_header チェインの中間でエラー → 後続は実行されず、先行は追加済み
let mut r = Response::new(200, "OK").unwrap();
let result = r.add_header("X-A", "1")?.add_header("", "bad"); // 空ヘッダー名は不正
assert!(result.is_err());
// result の &mut r 借用は assert! 以降使用されないため NLL により借用終了
drop(result); // 明示的に drop して借用を解放 (NLL 非依存でも動作するよう明示)
// X-A は追加済み
assert_eq!(r.get_headers("X-A"), vec!["1"]);

// String を所有する値がムーブされることの確認
let name = String::from("X-Custom");
let value = String::from("my-value");
let mut r = Response::new(200, "OK").unwrap();
r.add_header(name, value)?; // name, value はムーブされる
// コンパイル時: name, value は以降使用不可 (move)

// set_header のチェイン呼び出し
let mut r = Response::new(200, "OK").unwrap();
r.set_header("Content-Type", "text/plain")?
 .set_header("Content-Length", "0")?;
assert_eq!(r.get_header("Content-Type"), Some("text/plain"));

// set_body のチェイン呼び出し (set_body は infallible)
let mut r = Response::new(200, "OK").unwrap();
r.set_body(b"hello".to_vec())
 .set_omit_body(true);
assert!(r.body_bytes().is_some());
assert!(r.is_body_omitted());
```

#### Fuzz

`0017` 完了後の現行コードを基準に、以下の改修を行う。全 fuzz ターゲットの `add_header` 呼び出しは既に `.is_err()` / `?` / `.unwrap()` で Result を消費済みであり、`add_header` の戻り値型が `Result<(), E>` → `Result<&mut Self, E>` に変わってもコンパイル可能。追加の `let _ =` は不要。

`fuzz/fuzz_targets/fuzz_encode_response.rs`:

現行コード (L47-51):
```rust
let response = if body_present {
    response.body(body)
} else {
    response
};
```

`body_present: false` のパスで `body = None` を明示するため、`clear_body()` を呼ぶ形に変更する:
```rust
let mut response = if body_present {
    response.body(body)    // body() は self を消費し Self を返す
} else {
    response.clear_body(); // clear_body() の戻り値 &mut Self は ; で破棄され借用が終了する
    response               // 借用終了後なので response の move は可能
};
```
注: `clear_body()` の戻り値 `&mut Self` は文末の `;` で一時値が破棄され、`response` への可変借用が解放されるため、後続行の `response` の move が可能。

`fuzz/fuzz_targets/fuzz_request_response_helpers.rs`:

現行コード (L119-135) は `let response = response.body(body);` で `body` が空の場合 `Some(vec![])` になる。`body = None` パスのカバレッジは `Response::new` の初期状態で既に確保されており、本 issue での追加対応は不要。`body_present: bool` フィールドの追加も本 fuzz ターゲットの責務（ヘルパーメソッドの整合性検証）外のため不要。

`fuzz/fuzz_targets/fuzz_decoder_roundtrip.rs`:

現行コード (L134-137) は `has_body && !response_body.is_empty()` のときのみ `response.body(response_body.clone())` を呼び、それ以外は `Response::new` の初期状態 `body = None` のままである。`body = None` パスは既にカバー済みのため、本 issue での変更は不要。
```rust
// 現行コード (変更不要)
let response_body = fuzz_resp.body.clone();
let response = if has_body && !response_body.is_empty() {
    response.body(response_body.clone())
} else {
    response
};
```
注: `has_body` フラグは 1xx/204/304 の body 不可ステータスを表す。これらのステータスで `body = None` を維持するため `clear_body()` は不要 (`Response::new` の初期状態が `body = None`)。

`fuzz/fuzz_targets/fuzz_decoder_chunked.rs`:

現行コード (L215):
```rust
response.add_header("Transfer-Encoding", "chunked").unwrap();
```
戻り値型変更後も `.unwrap()` で正常動作する。変更不要。

`fuzz_encode_response.rs` の `omit_body` に関する改修:
`0017` 完了後の現行コード (L52):
```rust
let response = response.omit_body(omit_body);
```
本 issue で追加する `set_omit_body` に移行する:
```rust
response.set_omit_body(omit_body);
```
注: `let response = response.omit_body(omit_body)` は builder パターンで self を消費する。`response.set_omit_body(omit_body)` は `&mut self` を受け取り `&mut Self` を返すため、`let` の再束縛が不要になる。この変更は機能的に等価であり、`#[must_use]` 警告も発生しない (`&mut Self` の戻り値は破棄可能)。

#### Examples

`examples/http11_reverse_proxy/src/main.rs`: 全 4 箇所の `add_header` 呼び出し (L577, L581, L584, L588) は既に `?` 演算子で Result を消費済み。`add_header` の戻り値型が `Result<(), E>` → `Result<&mut Self, E>` に変わっても `?` は正常動作する。**変更不要。**

他の examples (http11_server, http11_server_io_uring, http11_client) は `Response` のビルダー呼び出しに `.unwrap()` が追加されている (0017 の変更)。本 issue では `impl Into` 化の影響確認のみで追加の変更は不要。`impl Into<String>` / `impl Into<Vec<u8>>` は既存の `&str` / `Vec<u8>` 呼び出しと互換である。

### Request 側について

本 issue は `Response` に限定する。`Request` の `add_header` 等も同様の問題を抱えているが、`0017` の Request 版 issue 完了後に別 issue で対応する。

## CHANGES.md

`## develop` に以下を追加する。CHANGES.md は着手時に確定した内容のみを記載し、条件分岐を含めないこと。

```
- [CHANGE] `Response::add_header` / `Response::set_header` の戻り値を `Result<&mut Self, EncodeError>` に変更しチェイン可能にする
  - 0017 で Result 化された両メソッドの戻り値型を `Result<(), E>` から `Result<&mut Self, E>` に変更する (戻り値型の変更は破壊的)
  - 0017 完了後の現行コードでは全呼び出し箇所が Result を消費済みのため、追加の `let _ =` は不要
  - @voluntas
- [UPDATE] `Response` の文字列・バイト列受け取り API を `impl Into<String>` / `impl Into<Vec<u8>>` に変更する
  - 対象: `new`, `with_version`, `header`, `add_header`, `set_header` (impl Into<String>), `body`, `set_body` (impl Into<Vec<u8>>)
  - 呼び出し側が `String` や `Vec<u8>` を所有している場合、ムーブで渡せるようになる
  - @voluntas
- [ADD] `Response::set_body` / `Response::clear_body` / `Response::without_body` を追加する
  - `set_body(&mut self, body)` は mutator でボディを設定する
  - `clear_body(&mut self)` は body を None に設定する
  - `without_body(self)` はビルダーで body を None に設定する
  - @voluntas
- [ADD] `Response::set_omit_body` を追加する
  - `is_body_omitted()` getter に対応する mutator
  - @voluntas
```

注: `set_omit_body` のエントリは pending/0018 が accept された場合に削除する。CHANGES.md には条件分岐を含めず、着手時に確定した単一のエントリを記載する。

## 検証方針

### 不変条件のテスト

- 新規単体テストで以下を確認する:
  - `clear_body` → `body_bytes()` が `None` を返す
  - `without_body` → `body_bytes()` が `None` を返す
  - `set_body(data)` → `body_bytes()` が `Some(data)` を返す
  - `set_omit_body(true)` → `is_body_omitted()` が `true` を返す
  - `add_header(a, b)?.add_header(c, d)?` のチェインが動作する
  - チェイン中間でバリデーションエラーが発生した場合、先行ヘッダーは追加済みで後続は追加されない
  - `set_header` のチェインが動作する
- PBT で以下を確認する:
  - `set_body` → `body_bytes()` のラウンドトリップ
  - `set_body` → `clear_body` → `body_bytes()` が `None`
  - `without_body` ビルダー → `body_bytes()` が `None`
  - `add_header` チェインで複数ヘッダーが正しく追加される
  - `impl Into<String>` に `&str` / `String` 両方を渡せる
  - `impl Into<Vec<u8>>` に `Vec<u8>` を渡せる

### 既存挙動の回帰確認

- 既存の単体テスト (`tests/test_encoder.rs` 等) が新 API に追従して green になる
- PBT (`prop_response.rs`, `prop_encoder.rs` 等) が新 API に追従して green になる
- fuzz ターゲット (全 12 個) が新 API に追従してコンパイル可能である
- 全 examples がコンパイルおよび実行可能である

### `#[must_use]` 警告がないことの確認

- `make clippy` が `-D warnings` で成功する (`unused_must_use` 警告が 0 件であること)
- `add_header` / `set_header` の全呼び出し箇所で戻り値が適切に処理されていること

### カバレッジ検証

```bash
cargo llvm-cov clean --workspace
cargo llvm-cov --no-report -p shiguredo_http11 --lib -- response
cargo llvm-cov --no-report -p shiguredo_http11 --test test_response
cargo llvm-cov --no-report -p pbt --test prop_response
cargo llvm-cov report
```

新規追加メソッド (`set_body`, `clear_body`, `without_body`, `set_omit_body`) の全分岐がカバーされていることを確認する。

## 受け入れ基準

- ブランチ名が `feature/change-response-builder-mutator-consistency` であること
- `make fmt && make clippy && make check && make test` がすべて成功する
- `add_header` の戻り値が `Result<&mut Self, EncodeError>` になっている
- `set_header` の戻り値が `Result<&mut Self, EncodeError>` になっている
- `Response::set_body` / `clear_body` / `without_body` が公開 API として存在する
- `Response::set_omit_body` が公開 API として存在する (pending/0018 が reject の場合)
- `new` / `with_version` / `header` / `add_header` / `set_header` が `impl Into<String>` を取る
- `body` / `set_body` が `impl Into<Vec<u8>>` を取る
- チェイン呼び出し (`response.add_header(a, b)?.add_header(c, d)?`) の単体テストが成功する
- `set_header` のチェイン呼び出しの単体テストが成功する
- 全 fuzz ターゲットが新 API に追従しコンパイル可能である (0017 完了後の現行コード基準)
- 全 examples がコンパイルおよび実行可能である
- 既存テスト・例が新 API に追従して green になる
- `body_bytes()` getter の命名が維持されている (改名しない)
