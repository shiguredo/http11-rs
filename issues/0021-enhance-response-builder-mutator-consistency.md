# 0021: Response のビルダーと mutator API を整備する

Created: 2026-05-06
Model: Opus 4.7 / DeepSeek V4 Pro

## 概要

`Response` のビルダー API と mutator API の欠落・非対称・冗長アロケーションを解消する。具体的には:

- mutator (`add_header`) の戻り値を `Result<&mut Self, EncodeError>` にしてチェイン可能にする
- ビルダー / mutator の対応が片寄っているフィールド (body, omit_body) について、両系統を対称に提供する
- 文字列・バイト列受け取りを `impl Into<String>` / `impl Into<Vec<u8>>` に変更し、呼び出し側が所有値をムーブ可能にする
- `set_header` の引数も同様に `impl Into<String>` 化する

破壊的変更を含むが、原則として戻り値を破棄する既存呼び出しはコンパイル可能な範囲に留める。ただし `Result<&mut Self, E>` の `#[must_use]` により、戻り値を破棄する呼び出しでは clippy 警告が発生するため、`let _ =` または `.ok();` による明示的な破棄が必要になる (詳細は後述)。

依存関係: `0017` (フィールド非公開化とバリデーション付き構築) の完了後に着手する。`0017` で `Response` のフィールドが非公開化され、`new` / `with_version` / `header` / `add_header` / `set_header` が `Result` を返すようになることを前提とする。本 issue はその上に、戻り値型の変更 (`&mut Self` 返却) と引数型の変更 (`impl Into`) を重ねる。

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
pub fn add_header(&mut self, name: &str, value: &str)
```

戻り値が `()` のため、`response.add_header(a, b).add_header(c, d)` のチェイン呼び出しができない。`0017` で `Result<(), EncodeError>` 返却に変わるが、`Result<&mut Self, EncodeError>` を返せば `?` 伝播で連続呼び出しが書ける。

### 問題 3: 引数の重複アロケーション

```rust
pub fn add_header(&mut self, name: &str, value: &str) {
    self.headers.push((name.to_string(), value.to_string()));
}
```

`&str` を受けて即 `to_string()` するため、呼び出し側が既に `String` を持っていてもムーブできず必ずクローンする。`impl Into<String>` で受ければ `String` ・ `&str` 両方に対応でき、`String` の場合はムーブで済む。

対象メソッド:
- `Response::new` (reason_phrase)
- `Response::with_version` (version, reason_phrase)
- `Response::header` (name, value)
- `Response::add_header` (name, value)
- `Response::body` (body: `Vec<u8>` → `impl Into<Vec<u8>>`)
- `Response::set_body` (同上)
- `Response::set_header` (name, value — `0017` で新設されるメソッド)

HTTP ヘッダー名・値は高々数十バイトの文字列であるため、この変更は致命的なボトルネックの解消ではないが、所有値をムーブ可能にすることで不必要なクローンを避けられる。`impl Into<String>` は Rust の標準的なパターンであり、API の表現力を損なわず、既存の `&str` 呼び出しも引き続きコンパイル可能である。

### 問題 4: `header` と `add_header` の命名が非対称

| 方向 | メソッド名 |
|---|---|
| ビルダー (`self`) | `header` |
| mutator (`&mut self`) | `add_header` |

どちらも `headers.push(...)` の動作であり、命名が非対称。ただし body 側では `body` (builder) / `set_body` (mutator) と `set_` プレフィックスで区別しており、header 側も builder は `header`、mutator は `add_header` で実質的な動作 (追加) を表現している。本 issue ではこの非対称性を認識した上で、破壊的変更を避けるため既存命名を維持する。body / omit_body の新規 mutator には `set_` プレフィックスを採用し、今後の命名規則を統一する。

### 問題 5: 0017 の `body` ゲッター / ビルダー名前衝突

0017 は `pub fn body(&self) -> Option<&[u8]>` (getter) と `pub fn body(mut self, body: Vec<u8>) -> Self` (builder) の両方を提案しているが、Rust では同一 impl ブロック内で同名メソッドを定義できない。この衝突は 0017 実装時に解決する必要がある。

**本 issue での解決方針**: builder 名 `body(self, ...)` は既存 API のため維持し、getter 側を `as_body(&self) -> Option<&[u8]>` に改名する。この命名は `Option::as_ref()` や `Vec::as_slice()` の Rust 慣習に従う。本 issue のコード例・テストはすべてこの命名で記述する。

### RFC 準拠の根拠

本 issue で変更する API 要素の根拠となる RFC 要件:

| API 要素 | 引用 RFC | 要件 |
|---|---|---|
| ヘッダー名形式 | RFC 9110 §5.1 | `field-name = token` 。1 文字以上の tchar 集合に制限される |
| ヘッダー値形式 | RFC 9110 §5.5 | field-value に CR, LF, NUL を含めてはならない。CTL 文字も無効 |
| ヘッダー名重複 | RFC 9110 §5.3 | 同一フィールド名の複数行生成は、そのフィールド定義が許容する場合を除き禁止 (MUST NOT)。Set-Cookie は例外 |
| reason-phrase 形式 | RFC 9112 §4 | `reason-phrase = 1*( HTAB / SP / VCHAR / obs-text )` 。少なくとも 1 文字必要 |
| reason-phrase 省略 | RFC 9110 §15.1 | reason-phrase は推奨値のみで、置換または省略可能 |
| HTTP-version 形式 | RFC 9112 §2.3 | `HTTP-version = HTTP-name "/" DIGIT "." DIGIT` (status-line 経由、ABNF 定義は §2.3) |
| status-code 形式 | RFC 9112 §4 | `status-code = 3DIGIT` |
| body なしレスポンス | RFC 9110 §6.4.1 | HEAD / 1xx / 204 / 304 / CONNECT 2xx レスポンスは content を含まない |
| body 長決定 | RFC 9112 §6.3 | HEAD / 1xx / 204 / 304 / CONNECT 2xx は空行で終端し、message body は存在しない |
| Content-Length 禁止 | RFC 9110 §8.6 | 1xx / 204 / CONNECT 2xx レスポンスでは Content-Length を送信してはならない (MUST NOT) |

注: ヘッダー名・値のバリデーションは `0017` で `is_valid_header_name` / `is_valid_field_value` を用いて実装済み。本 issue はバリデーション内容を変更しない。

## 対応方針

### 影響範囲一覧

| ファイル | 種別 | 内容 |
|---|---|---|
| `src/response.rs` | 主要変更 | `add_header` 戻り値変更、body/omit_body mutator 追加、`impl Into` 化、`without_body` 追加、`set_header` の `impl Into` 化 |
| `pbt/tests/prop_response.rs` | 修正 | 全テストのシグネチャ変更 (`Result` 処理 + `impl Into` 対応) |
| `pbt/tests/prop_encoder.rs` | 修正 | `Response` ビルダー利用箇所の追従確認 |
| `fuzz/fuzz_targets/fuzz_encode_response.rs` | 修正 | `add_header` 戻り値処理追加、フィールド直接代入 → mutator API 移行 |
| `fuzz/fuzz_targets/fuzz_request_response_helpers.rs` | 修正 | 同上 |
| `fuzz/fuzz_targets/fuzz_decoder_roundtrip.rs` | 修正 | `add_header` 戻り値処理追加、`response.body` 直接代入 → `set_body` 移行 |
| `fuzz/fuzz_targets/fuzz_decoder_chunked.rs` | 修正 | `add_header` 戻り値処理追加 |
| `examples/http11_reverse_proxy/src/main.rs` | 修正 | `add_header` 4 箇所の戻り値処理追加 |
| `examples/http11_server/src/main.rs` | 修正 | `Response` 構築箇所の追従確認 |
| `examples/http11_server_io_uring/src/main.rs` | 修正 | 同上 |
| `examples/http11_client/src/main.rs` | 修正 | 同上 |
| `tests/test_encoder.rs` | 修正 | `Response` ビルダー呼び出しの追従確認 |
| `tests/test_response.rs` | 修正 | `clear_body` / `without_body` / チェイン動作の単体テストを追記 (`0017` で作成されるファイルに追記) |
| `CHANGES.md` | 修正 | `## develop` にエントリ追加 |

### src/response.rs

#### 1. `add_header` のチェイン化

```rust
/// ヘッダーを追加
///
/// 名前は RFC 9110 §5.1 の token (1*tchar)、
/// 値は RFC 9110 §5.5 の field-value (CR/LF 不可) を満たす必要がある。
/// バリデーション成功後にヘッダーが追加される (失敗時は self に変更なし)。
pub fn add_header(
    &mut self,
    name: impl Into<String>,
    value: impl Into<String>,
) -> Result<&mut Self, EncodeError> {
    // バリデーション → 成功時のみ push (順序保証)
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
/// HEAD レスポンス (RFC 9110 §6.4.1) 等で、Content-Length は表現長として
/// 残しつつメッセージボディを送信しない場合に使用する。
pub fn set_omit_body(&mut self, omit: bool) -> &mut Self {
    self.omit_body = omit;
    self
}
```

`body = None` (ボディなし、Content-Length 自動付与なし) と `body = Some(vec![])` (明示的空ボディ、Content-Length: 0 自動付与) の区別:
- `set_body(Vec::new())` → `body = Some(vec![])` (空ボディ)
- `clear_body()` / `without_body()` → `body = None` (ボディ意図なし)

この区別は `closed/0004-change-request-response-body-optional.md` の設計判断に基づき、型レベルで表現済み。

#### 3. body ビルダーの `without_body` 追加

```rust
/// ボディなしを明示 (ビルダーパターン)
///
/// `body = None` に設定する。builder チェイン中に `body()` を呼んだ後で
/// ボディを取り消す場合に使用する。
pub fn without_body(mut self) -> Self {
    self.body = None;
    self
}
```

`no_body` ではなく `without_body` を採用する理由: `no_body` は `omit_body` (ボディ送信抑止) と区別がつきにくい。`without_body` は「ボディを持たない」を明確に表現し、`omit_body` との混同を避ける。

mutator 側は `clear_body` を採用する。Rust の慣習 (`Vec::clear()`, `String::clear()`) に従い、mutator での「内容を空にする/概念を削除する」操作は `clear_` プレフィックスが自然。builder 側は `with_` / `without_` が自然であり、`body()` / `without_body()` の対称性は Rust の builder パターンとして許容範囲内。`set_body` / `clear_body` の mutator ペアも同様に自然な対称性を持つ。

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

#### 5. `set_header` の `impl Into<String>` 化

`0017` で新設される `set_header` も同様に `impl Into<String>` 化する:

```rust
pub fn set_header(&mut self, name: impl Into<String>, value: impl Into<String>) -> Result<&mut Self, EncodeError>
```

注: `set_header` は本 issue で `&mut Self` を返すように変更する (`0017` では `Result<(), EncodeError>` で定義される)。ヘッダー操作系の mutator はすべて `Result<&mut Self, EncodeError>` を返すことでチェイン可能にする。

#### 6. `#[must_use]` と既存コードの対応

`Result<T, E>` には `#[must_use]` が付与されている。`add_header` / `set_header` の戻り値を `Result<(), E>` から `Result<&mut Self, E>` に変更しても `#[must_use]` の有無は変わらないが、`0017` 適用時点で既存の `add_header` 呼び出しは戻り値破棄により `unused_must_use` 警告が発生する。本 issue では追加の警告を発生させないが、`0017` のフォローアップとして以下の対応が必要であることを明記する:

| 影響ファイル | 対応内容 |
|---|---|
| `fuzz/fuzz_targets/fuzz_encode_response.rs:38` | `let _ = response.add_header(name, value);` |
| `fuzz/fuzz_targets/fuzz_request_response_helpers.rs:130` | 同上 |
| `fuzz/fuzz_targets/fuzz_decoder_roundtrip.rs:121` | 同上 |
| `fuzz/fuzz_targets/fuzz_decoder_chunked.rs:215` | 同上 |
| `examples/http11_reverse_proxy/src/main.rs:572,576,579,583` | 同上 (または `.ok();`) |

注: fuzz ターゲットのフィールド直接代入 (`response.body = Some(body)`, `response.omit_body = omit_body`) は `0017` の影響でコンパイル不可になる。本 issue で追加する `set_body` / `set_omit_body` に移行する:

```rust
// 旧
response.body = if body_present { Some(body) } else { None };
response.omit_body = omit_body;

// 新
if body_present {
    response.set_body(body);
}
response.set_omit_body(omit_body);
// body_present: false の場合は clear_body() を呼ぶか初期状態 (body = None) のままにする
```

#### 0017 / 0018 との依存関係と命名の取り扱い

`pending/0018` (omit_body 撤去提案) の成否によって `set_omit_body` の有無が変わる。本 issue では 0018 が pending のままでも実装可能なように、以下の分岐方針を取る:

- `0018` が **reject された場合**: `set_omit_body` をそのまま残す
- `0018` が **accept され `omit_body` が撤去された場合**: `set_omit_body` を省く。`without_body` / `clear_body` は `omit_body` 撤去後も意味を持つ (`body = None` 設定) ため残す

`0017` の実装結果による命名の影響:
- `0017` で getter が `is_body_omitted()` になる場合 (0017 の現行提案) → `set_omit_body()` のまま (getter/setter で命名統一不要、`is_` は問い合わせ、`set_` は操作で自然)
- `0017` で getter が `omit_body()` になる場合 → `set_omit_body()` のまま (変更なし)
- `0017` で getter が `omit_body_flag()` になる場合 → `set_omit_body` を `set_omit_body_flag()` に改名する

この判断は `0017` 実装完了時点で確定させる。本 issue の着手時点で、0017 の完了状態を確認し命名を確定させること。

### tests / pbt / fuzz / examples

#### PBT (`pbt/tests/prop_response.rs`)

`0017` により全テストが `Result` 処理の追加を必要とする。本 issue ではさらに `impl Into` 化の対応と、新規 API の PBT を追加する:

既存テストの修正方針:
- `Response::new(code, "OK")` → `Response::new(code, "OK").unwrap()`
- `.header("Content-Type", "text/html")` → `.header("Content-Type", "text/html").unwrap()`
- フィールド直接アクセス (`response.headers.len()`) → getter 経由 (`response.headers().len()` — `0017` の変更)
- `.body(data.clone())` → 変更不要 (`impl Into<Vec<u8>>` は `Vec<u8>` をそのまま受け付ける)

新規追加 PBT:
- `set_body` → `as_body()` (getter) のラウンドトリップ
- `set_body` → `clear_body` → `as_body()` が `None` になること
- `without_body` ビルダー → `as_body()` が `None` になること
- `add_header` チェイン: `response.add_header(a, v)?.add_header(b, w)?` で両方のヘッダーが追加されること
- `add_header` → `clear_body` → `set_body` の mutator チェイン
- `impl Into<String>` に `&str` と `String` の両方を渡せること (Strategy で両方生成)
- `impl Into<Vec<u8>>` に `Vec<u8>` を渡せること

#### 単体テスト (`tests/test_response.rs`)

`0017` で新設されるファイルに以下を追記する。テスト関数は `Result<(), EncodeError>` または `Result<(), Box<dyn Error>>` を返すシグネチャが必要 (`?` 演算子使用のため):

```rust
// clear_body で body が None になる
let mut r = Response::new(200, "OK").unwrap();
r.set_body(b"data".to_vec());
assert!(r.as_body().is_some());
r.clear_body();
assert!(r.as_body().is_none());

// without_body ビルダーで body が None になる
let r = Response::new(200, "OK").unwrap()
    .body(b"data".to_vec())
    .without_body();
assert!(r.as_body().is_none());

// set_body で body が設定される
let mut r = Response::new(200, "OK").unwrap();
r.set_body(b"hello".to_vec());
assert_eq!(r.as_body(), Some(b"hello".as_slice()));

// set_omit_body で omit_body が設定される
let mut r = Response::new(200, "OK").unwrap();
r.set_omit_body(true);
assert!(r.is_body_omitted());

// add_header チェイン呼び出し
let mut r = Response::new(200, "OK").unwrap();
r.add_header("X-A", "1")?.add_header("X-B", "2")?;
assert_eq!(r.get_headers("X-A").collect::<Vec<_>>(), vec!["1"]);
assert_eq!(r.get_headers("X-B").collect::<Vec<_>>(), vec!["2"]);

// add_header チェインの中間でエラー → 後続は実行されず、先行は追加済み
let mut r = Response::new(200, "OK").unwrap();
let result = r.add_header("X-A", "1")?.add_header("", "bad"); // 空ヘッダー名は不正
assert!(result.is_err());
// X-A は追加済み
assert_eq!(r.get_headers("X-A").collect::<Vec<_>>(), vec!["1"]);

// String を所有する値がムーブされることの確認
let name = String::from("X-Custom");
let value = String::from("my-value");
let mut r = Response::new(200, "OK").unwrap();
r.add_header(name, value)?; // name, value はムーブされる
// コンパイル時: name, value は以降使用不可 (move)
```

#### Fuzz

各 fuzz ターゲットの改修内容:

| ファイル | 行 | 旧コード | 新コード |
|---|---|---|---|
| `fuzz_encode_response.rs` | 38 | `response.add_header(name, value);` | `let _ = response.add_header(name, value);` |
| `fuzz_encode_response.rs` | 40-41 | `response.body = if body_present { Some(body) } else { None };` | `if body_present { response.set_body(body); } else { response.clear_body(); }` |
| `fuzz_encode_response.rs` | 41 | `response.omit_body = omit_body;` | `response.set_omit_body(omit_body);` |
| `fuzz_request_response_helpers.rs` | 130 | `response.add_header(name, value);` | `let _ = response.add_header(name, value);` |
| `fuzz_request_response_helpers.rs` | 132 | `response.body = Some(body);` | `response.set_body(body);` |
| `fuzz_decoder_roundtrip.rs` | 121 | `response.add_header(name, value);` | `let _ = response.add_header(name, value);` |
| `fuzz_decoder_roundtrip.rs` | 130 | `response.body = Some(body.clone());` | `response.set_body(body.clone());` |
| `fuzz_decoder_chunked.rs` | 215 | `response.add_header("Transfer-Encoding", "chunked");` | `let _ = response.add_header("Transfer-Encoding", "chunked");` |

#### Examples

`examples/http11_reverse_proxy/src/main.rs` の `response_for_headers.add_header(...)` 呼び出し (4 箇所: L572, L576, L579, L583) に `let _ = ` または `.ok();` を追加する。examples は「お手本」であるため、可能な箇所ではチェイン形式に書き換えることが望ましいが、必須ではない。

他の examples (http11_server, http11_server_io_uring, http11_client) は `Response` のビルダー呼び出しに `.unwrap()` が追加される (`0017` の変更)。本 issue では `impl Into` 化の影響確認のみで追加の変更は不要。

### Request 側について

本 issue は `Response` に限定する。`Request` の `add_header` 等も同様の問題を抱えているが、`0017` の Request 版 issue 完了後に別 issue で対応する。

## CHANGES.md

`## develop` に以下を追加する:

```
- [UPDATE] `Response::add_header` / `Response::set_header` の戻り値を `Result<&mut Self, EncodeError>` に変更しチェイン可能にする
  - 0017 で Result 化された両メソッドの戻り値型を `Result<(), E>` から `Result<&mut Self, E>` に変更する
  - 既存の戻り値破棄呼び出しは `#[must_use]` 警告が発生するため `let _ =` を追加する
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
  - pending/0018 が reject された場合はこのエントリを維持する
  - pending/0018 が accept された場合はこのエントリを適用しない
  - @voluntas
```

## 検証方針

### 不変条件のテスト

- 新規単体テストで以下を確認する:
  - `clear_body` → `as_body()` が `None` を返す
  - `without_body` → `as_body()` が `None` を返す
  - `set_body(data)` → `as_body()` が `Some(data)` を返す
  - `set_omit_body(true)` → `is_body_omitted()` が `true` を返す
  - `add_header(a, b)?.add_header(c, d)?` のチェインが動作する
  - チェイン中間でバリデーションエラーが発生した場合、先行ヘッダーは追加済みで後続は追加されない
- PBT で以下を確認する:
  - `set_body` → getter のラウンドトリップ
  - `set_body` → `clear_body` → `as_body()` が `None`
  - `without_body` ビルダー → `as_body()` が `None`
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

- `make fmt && make clippy && make check && make test` がすべて成功する
- `add_header` の戻り値が `Result<&mut Self, EncodeError>` になっている
- `set_header` の戻り値が `Result<&mut Self, EncodeError>` になっている
- `Response::set_body` / `clear_body` / `without_body` が公開 API として存在する
- `Response::set_omit_body` が公開 API として存在する (pending/0018 が reject の場合)
- `new` / `with_version` / `header` / `add_header` / `set_header` が `impl Into<String>` を取る
- `body` / `set_body` が `impl Into<Vec<u8>>` を取る
- チェイン呼び出し (`response.add_header(a, b)?.add_header(c, d)?`) の単体テストが成功する
- 全 fuzz ターゲットが新 API に追従しコンパイル可能である
- 全 examples がコンパイルおよび実行可能である
- 既存テスト・例が新 API に追従して green になる
