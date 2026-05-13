# 0023: Response と Request と HttpHead の RFC 根拠と委譲メソッドの doc を補強する

Created: 2026-05-06
Completed: 2026-05-07
Model: Opus 4.7

## 概要

`Response`、`Request`、および `HttpHead` トレイトの委譲メソッド群について、RFC の節番号引用、戻り値の意味論、実装の挙動を doc コメントに明記する。AGENTS.md「資料由来の機能を実装する場合は、根拠資料名、節番号、将来変更される可能性をコードコメントで明記すること」に追従する。

破壊的変更なし。doc のみの改善。

- `0017` (Response フィールド非公開化) は完了済み。本 issue は完了後のコード状態を前提とする
- `0021` (ビルダー/mutator API 一貫化) の完了後に `set_omit_body` が追加されるが、本 issue の doc 補強は既存メソッドのみを対象とし、`set_omit_body` の doc は `0021` 側で RFC 節番号を含めて実装する。`0021` の現行提案では `set_omit_body` の doc に `RFC 9110 Section 6.4.1` が正しく記載されており、本 issue で指摘する必要はない
- `pending/0018` (omit_body 撤去) が将来 accept された場合、本 issue で記載する `omit_body` 関連の doc は `0018` 側で撤去される
- `0025` (Request フィールド非公開化) は未着手だが、本 issue は doc のみの変更のため競合しない
- 実装後、`0025` 完了時に getter 経由になる等の調整が必要な場合は `0025` 側で doc を更新する

本 issue は `0021` より先に、または同時に着手してよいが、提出する PR は `0021` 完了後のコードベースに対する方が行番号の競合を避けられる。

## 根拠

### 問題 1: RFC 節番号が記載されていない

AGENTS.md「資料由来の機能を実装する場合は、根拠資料名、節番号、将来変更される可能性をコードコメントで明記すること」のルールに対し、以下が違反している（0017 完了後のコード状態で確認）:

**src/response.rs の doc 不在箇所**:

| 対象 | 行 | 状態 |
|---|---|---|
| `omit_body` フィールド (`omit_body: bool`) | L32 | フィールド自体の doc コメントがなく、構造体冒頭 doc (L18-20) にも RFC 節番号なし |
| `is_keep_alive` メソッド | L348-355 | "RFC 9110" への言及はあるが節番号なし |
| `is_chunked` メソッド | L362-368 | "RFC 9112" への言及はあるが節番号なし |
| `content_length` メソッド | L357-360 | doc が「`Content-Length` ヘッダーの値を取得」のみで RFC 節番号なし |
| `connection` メソッド | L343-346 | doc が「`Connection` ヘッダーの値を取得」のみで RFC 節番号なし |
| `is_body_omitted` メソッド | L313-316 | doc が「ボディ送信抑止フラグを取得 (HEAD レスポンス用)」のみで RFC 節番号なし |

注: `omit_body` メソッド (L208-215) には既に `RFC 9110 Section 9.3.2` が記載されている。本 issue ではこれに `RFC 9110 Section 6.4.1` を追加する。

**src/request.rs の doc 不在箇所**: `response.rs` と同一の委譲メソッド群 (`connection`, `is_keep_alive`, `content_length`, `is_chunked`。L93-118) が同一の問題を抱えている。本 issue で同時に補強する。

**補足: `get_header` / `get_headers` / `has_header` について**: `response.rs` と `request.rs` には `get_header` / `get_headers` / `has_header` の 3 メソッドも `HttpHead` への委譲メソッドとして存在するが、これらは HTTP 意味論に特有の判定ロジックを持たず単なる検索・集約の薄いラッパーであり、AGENTS.md が要求する「資料由来の機能」には該当しないため本 issue の対象外とする。ただし現状これらのメソッドには doc コメント自体がないため、RFC 節番号とは別に基本的な説明 doc を追加することは別 issue で検討する。

**src/decoder/head.rs (HttpHead トレイト) の doc 不在箇所**:

| 対象 | 行 | 状態 |
|---|---|---|
| `connection` メソッド | L39-42 | doc が「`Connection` ヘッダーの値を取得」のみで RFC 節番号なし |
| `content_length` メソッド | L74-78 | doc が「`Content-Length` ヘッダーの値を取得」のみで RFC 節番号なし |
| `is_keep_alive` メソッド | L44-47 | doc に `RFC 9110 Section 9.1` とあるが、Section 9.1 は "Overview of Methods" であり誤った節番号（後述） |
| `is_chunked` メソッド | L80-105 | 既に `RFC 9112 Section 6.3` が記載されている。内容は十分だが整合性のために再確認する |

**head.rs `is_keep_alive` の誤った RFC 節番号**: 既存 doc (L44-47) は `/// RFC 9110 Section 9.1: 複数の Connection ヘッダーはリストとして結合して処理する。` とあるが、RFC 9110 Section 9.1 は "Overview of Methods" (method token の定義、メソッド一覧表の導入部) であり、Connection ヘッダー処理とは無関係である。正しい参照先は RFC 9110 Section 5.3 (複数ヘッダー行の結合規則) および RFC 9110 Section 7.6.1 (Connection ヘッダー定義) である。本 issue で修正する。これは doc 上のバグに相当する。

### 問題 2: 戻り値の意味論が doc 不在

`content_length` と `connection` について、以下が doc から読み取れない:

- `HttpHead::content_length` は `get_header` → `.find()` で **最初の `Content-Length` ヘッダー値のみ** を `u64` にパースして返す。複数ヘッダーが存在しても後続は無視される。RFC 9110 Section 5.3 により、`Content-Length` の複数フィールド行生成はそもそも禁止されているため、最初の 1 つだけを見る挙動は妥当である
- `HttpHead::connection` は `get_header("Connection")` で **最初の `Connection` ヘッダー値全体** をそのままの `&str` で返す。カンマ区切りトークンリストの分割は行わない。トークン判定が必要な場合は呼び出し側で `split(',')` する必要がある

### 問題 3: `is_keep_alive` の doc が不正確

```rust
/// HTTP/1.1 ではデフォルトでキープアライブ
/// HTTP/1.0 では Connection: keep-alive が必要
```

実際の `HttpHead::is_keep_alive` 実装 (`src/decoder/head.rs:48-72`) は:
- `close` トークンが最優先: いずれかの `Connection` ヘッダーに `close` があれば即座に `false`。`keep-alive` が同時に存在しても `close` が優先される
- `close` がない場合、`keep-alive` トークンがあれば `true`
- どちらもない場合、`version.ends_with("/1.1")` で判定

doc はこの判定ロジックの全体像を反映していない。特に HTTP/1.1 でも `Connection: close` で keep-alive が無効になる点が欠落している。

注: RFC 9112 Section 9.3 の HTTP/1.0 + keep-alive の持続条件には「recipient is not a proxy OR the message is a response」という proxy に関する追加条件があるが、`is_keep_alive` 実装はこの区別を行っていない。これは上位層で判断すべきという設計判断によるものであり、本 issue では doc に「proxy 条件は上位層の責務」と注記する。

### 問題 4: `is_chunked` の doc が実装と RFC 節番号で不整合

`Response::is_chunked` の doc (L362-368) には `(RFC 9112)` とのみ書かれているが、委譲先 `HttpHead::is_chunked` (L82) には `RFC 9112 Section 6.3` と明記されている。Response 側の doc もこれに合わせて節番号を追記する必要がある。

## 対応方針

### src/response.rs

#### 1. `omit_body` フィールドの doc 追加

フィールドは非公開 (`omit_body: bool`) で `cargo doc` に表示されないため、内部開発者向けの `//` コメントとして RFC 根拠を明記する。構造体冒頭 doc (L18-20) の `omit_body` に関する記述にも `RFC 9110 Section 6.4.1` を追記し、公開ドキュメント側からも RFC 根拠がわかるようにする。

フィールドの `//` コメント:
```rust
// ボディ送信を抑止するフラグ (HEAD レスポンス用)
//
// HEAD レスポンスはメッセージボディを送信しない (RFC 9110 Section 9.3.2 MUST NOT /
// RFC 9110 Section 6.4.1 "never include content") 。
// 1xx/204/304 はエンコーダーが自動的にボディを抑止するため、本フラグの設定は不要。
// `pub fn omit_body(omit: bool)` 経由でのみ設定可能。
//
// 注: pending/0018 で encoder 側のフラグへの移譲が検討されており、
// 本フィールドは将来撤去される可能性がある。
omit_body: bool,
```

構造体冒頭 doc (L18-20) の修正:
```rust
/// `omit_body` は body の有無とは直交する。HEAD レスポンスのように
/// メッセージボディを送らない場合に使う
/// (RFC 9110 Section 9.3.2 / RFC 9110 Section 6.4.1) 。
/// `Content-Length` は表現長として残す。
///
/// 注: 1xx/204/304 はエンコーダーが自動的にボディを抑止するため
/// `omit_body` の設定は不要。
/// 注: 304 は 1xx/204 と異なり Transfer-Encoding / Content-Length ヘッダーの
/// 設定が拒否されないが、ボディ送出自体は抑止される (encoder.rs の
/// `response_status_has_body` が false を返す) 。
/// また pending/0018 で encoder 側への移譲が検討されており、将来撤去される可能性がある。
```

#### 2. `omit_body` メソッドの doc 補強

既存の `/// HEAD レスポンス (RFC 9110 Section 9.3.2) で使用する。` を以下のように修正する:

```rust
/// ボディ送信を抑止する (ビルダーパターン)
///
/// HEAD リクエストへのレスポンスでメッセージボディを送信しない場合に使用する
/// (RFC 9110 Section 9.3.2 / RFC 9110 Section 6.4.1) 。
///
/// 1xx/204/304 レスポンスはエンコーダーが自動的にボディを抑止するため、
/// 本メソッドの呼び出しは不要。
///
/// `body` に非空データが設定されている場合、Content-Length は
/// body 長から自動付与される (ただしボディ実体は送信されない) 。
/// `body: Some(vec![])` の場合は Content-Length の自動付与も抑止される
/// (encoder.rs `should_auto_emit_content_length_for_response` 参照) 。
/// `body: None` の場合は Content-Length の自動付与も抑止される。
/// 任意の Content-Length を指定したい場合は、本メソッド呼び出し後に
/// `header("Content-Length", value)?` で手動設定する (0021 完了後は
/// `header` が `Result` を返すため `?` が必要) 。
```

#### 3. `is_body_omitted` メソッドの doc 補強

```rust
/// ボディ送信抑止フラグを取得 (HEAD レスポンス用)
///
/// RFC 9110 Section 6.4.1 / RFC 9110 Section 9.3.2 に基づき、
/// HEAD リクエストへのレスポンスでメッセージボディを送信しない場合に `true` を返す。
/// 1xx/204/304 等、エンコーダーが自動的にボディを抑止するレスポンスでは
/// 本フラグは常に `false` のままでも問題ない。
```

#### 4. `is_keep_alive` の doc 修正

```rust
/// キープアライブ接続かどうかを判定
///
/// 判定ロジックは `Connection` ヘッダーのトークンリストを評価した後、
/// プロトコルバージョンにフォールバックする:
///
/// - RFC 9112 Section 9.3: 持続性の判定基準
/// - RFC 9112 Section 9.6: close connection option の定義
/// - RFC 9110 Section 7.6.1: Connection ヘッダーの定義
/// - RFC 9110 Section 5.3: 複数ヘッダー行の結合規則
///
/// 判定順序:
///
/// 1. `Connection` ヘッダーのいずれかに `close` トークンが存在 → `false`
///    (`keep-alive` が同時に存在しても `close` が優先される)
/// 2. `Connection` ヘッダーのいずれかに `keep-alive` トークンが存在 → `true`
/// 3. それ以外 → `version` 文字列が `/1.1` で終わる場合のみ `true`
///
/// `Connection` ヘッダーはカンマ区切りトークンリストとして扱う
/// (RFC 9110 Section 7.6.1) 。
///
/// 注: HTTP/1.1 でも `Connection: close` が指定された場合は keep-alive にならない。
/// HTTP/1.0 で `Connection: keep-alive` がない場合も keep-alive にならない。
/// RFC 9112 Section 9.3 の HTTP/1.0 keep-alive 持続に含まれる proxy 条件
/// (recipient is not a proxy OR message is a response) は本メソッドでは区別しない。
/// これは上位層の責務である。
///
/// 詳細は委譲先 `HttpHead::is_keep_alive` を参照。
```

#### 5. `is_chunked` の doc 修正

```rust
/// Transfer-Encoding が chunked かどうかを判定 (RFC 9112 Section 6.3)
///
/// 全 `Transfer-Encoding` ヘッダーを走査し、RFC 9110 Section 5.3 に従い
/// 単一のトークンリストとして扱い、最後のトークンが `chunked` かどうかを確認する。
/// RFC 9112 Section 6.3 #4 の "chunked transfer coding is the final encoding" に基づく。
///
/// 例:
/// - `Transfer-Encoding: chunked` → `true`
/// - `Transfer-Encoding: gzip, chunked` → `true`
/// - `Transfer-Encoding: chunked, gzip` → `false`
///
/// 詳細は委譲先 `HttpHead::is_chunked` を参照。
```

#### 6. `content_length` の doc 追加

```rust
/// `Content-Length` ヘッダーの値を取得
/// (RFC 9110 Section 8.6 / RFC 9112 Section 6.2)
///
/// 最初の `Content-Length` ヘッダー値を `u64` としてパースして返す。
/// 複数ヘッダーが存在しても最初の値のみ参照する
/// (RFC 9110 Section 5.3 により、`Content-Length` の複数フィールド行生成は
/// そもそも禁止されている) 。
///
/// 値がパース不能な場合は `None` を返す。
///
/// 注: `Content-Length` の型は `u64` で、RFC 9110 Section 8.6 の
/// 「整数変換オーバーフロー防止 (Section 17.5)」要件に基づく。
/// RFC 9110 Section 17.5 (Attacks via Protocol Element Length) は
/// 算術オーバーフロー・DoS の一般的脅威を論じている。
///
/// Transfer-Encoding と Content-Length の排他関係 (RFC 9112 Section 6.1:
/// MUST NOT send Content-Length in any message that contains Transfer-Encoding)
/// は本メソッドの責務外であり、呼び出し側で判定する。
///
/// 詳細は委譲先 `HttpHead::content_length` を参照。
```

#### 7. `connection` の doc 追加

```rust
/// Connection ヘッダーの値を取得 (RFC 9110 Section 7.6.1)
///
/// 最初の `Connection` ヘッダー値をそのままの `&str` で返す。
/// カンマ区切りトークンリストの分割は行わない。
/// `close` / `keep-alive` 等のトークン判定は `is_keep_alive()` が行う。
/// 戻り値から自前でトークン分割する場合は `split(',')` を使用すること。
///
/// `Connection` ヘッダーが存在しない場合は `None` を返す。
```

### src/request.rs

`request.rs` の以下のメソッドに、`response.rs` と同一の doc を適用する。委譲先 (`HttpHead`) が同一であるため、doc の内容も同一でよい:

- `is_keep_alive` (L98-105): `response.rs` の `is_keep_alive` の doc (上記 4) と同一の内容
- `is_chunked` (L112-118): `response.rs` の `is_chunked` の doc (上記 5) と同一の内容
- `content_length` (L107-109): `response.rs` の `content_length` の doc (上記 6) と同一の内容
- `connection` (L93-95): `response.rs` の `connection` の doc (上記 7) と同一の内容

注: Request 側の `Request::new` や `with_version` は本 issue の対象外。これらは `0025` でバリデーション付き構築に変更される際に doc が整備される。また `0025` で `is_valid_method` / `is_valid_request_target` が追加されるため、本 issue で先回りして RFC 節番号を入れても `0025` の実装内容と矛盾する可能性がある。

### src/decoder/head.rs (HttpHead トレイト)

委譲元 (`Response` / `Request`) の doc に「詳細は委譲先を参照」と記載する方針に合わせ、`HttpHead` トレイト側の doc にも判定ロジックの詳細を記載する。委譲元と同じ情報量を保ち、「委譲元が詳細・委譲先が簡素」という情報量の逆転を避ける。

#### `connection` メソッドの doc 追加

```rust
/// Connection ヘッダーの値を取得 (RFC 9110 Section 7.6.1)
///
/// 最初の `Connection` ヘッダー値をそのままの `&str` で返す。
/// カンマ区切りトークンリストの分割は行わない。
/// 戻り値から自前でトークン分割する場合は `split(',')` を使用すること。
/// `Connection` ヘッダーが存在しない場合は `None` を返す。
fn connection(&self) -> Option<&str> {
```

#### `content_length` メソッドの doc 追加

```rust
/// Content-Length ヘッダーの値を取得
/// (RFC 9110 Section 8.6 / RFC 9112 Section 6.2)
///
/// 最初の `Content-Length` ヘッダー値を `u64` としてパースして返す。
/// RFC 9110 Section 5.3 により複数ヘッダー行の生成は禁止されているため、
/// 最初の値のみを参照する。
/// パース不能な場合は `None` を返す。
fn content_length(&self) -> Option<u64> {
```

#### `is_keep_alive` メソッドの doc 修正（重要）

既存 doc (L44-47) の `RFC 9110 Section 9.1` は誤り（Section 9.1 は "Overview of Methods" であり Connection ヘッダー処理とは無関係）。正しい節番号に修正し、委譲元と同等レベルの判定ロジック詳細を記載する:

```rust
/// キープアライブ接続かどうかを判定
///
/// 判定ロジックは `Connection` ヘッダーのトークンリストを評価した後、
/// プロトコルバージョンにフォールバックする:
///
/// - RFC 9112 Section 9.3: 持続性の判定基準
/// - RFC 9112 Section 9.6: close connection option の定義
/// - RFC 9110 Section 7.6.1: Connection ヘッダーの定義
/// - RFC 9110 Section 5.3: 複数ヘッダー行の結合規則
///
/// 判定順序:
///
/// 1. `Connection` ヘッダーのいずれかに `close` トークンが存在 → `false`
///    (`keep-alive` が同時に存在しても `close` が優先される)
/// 2. `Connection` ヘッダーのいずれかに `keep-alive` トークンが存在 → `true`
/// 3. それ以外 → `version` 文字列が `/1.1` で終わる場合のみ `true`
///
/// 注: HTTP/1.1 でも `Connection: close` が指定された場合は keep-alive にならない。
/// HTTP/1.0 で `Connection: keep-alive` がない場合も keep-alive にならない。
/// RFC 9112 Section 9.3 の HTTP/1.0 keep-alive 持続に含まれる proxy 条件
/// (recipient is not a proxy OR message is a response) は本メソッドでは区別しない。
/// これは上位層の責務である。
fn is_keep_alive(&self) -> bool {
```

#### `is_chunked` メソッドの doc 確認

既存 doc (L80-105) は既に `RFC 9112 Section 6.3` を明記しており、本 issue の方針と一致する。変更不要。

### doctest について

追加しない。理由:
- `is_keep_alive` 等の挙動は委譲先 `HttpHead` のデフォルト実装に依存しており、トレイトの PBT で検証済み
- `0017` 完了後の API (`Response::new` が `Result` を返す) で doctest を書く必要があるが、API シグネチャは今後も変更される可能性がある
- Doc の正確性はコードレビューと `cargo doc` の警告チェックで担保する

### テストへの影響

doc のみの変更のため、PBT / 単体 / Fuzzing の追加は不要。既存テスト (`tests/test_response.rs`, `pbt/tests/prop_response.rs`, `tests/test_encoder.rs` 等) への影響もない。

## CHANGES.md

`## develop` セクションの `### misc` サブセクションに以下を追加する:

```
- [FIX] `HttpHead::is_keep_alive` の doc 内の誤った RFC 節番号 (Section 9.1 → Section 9.3 / Section 7.6.1) を修正する
  - @voluntas
- [UPDATE] `Response`、`Request`、`HttpHead` の委譲メソッドの doc に RFC 節番号を明記する
  - 対象メソッド: `omit_body` / `is_body_omitted` / `is_keep_alive` / `is_chunked` / `content_length` / `connection`
  - `is_keep_alive` の doc に判定ロジックの全体像 (`close` 優先、version フォールバック) を明記し、RFC 9112 Section 9.6 (close option) の参照を追加する
  - `content_length` の doc に「最初のヘッダー値のみを返す」挙動と RFC 9110 Section 17.5 の整数変換オーバーフロー防止要件を明記する
  - `connection` の doc に「そのままの &str で返し、トークン分割は行わない」挙動を明記する
  - `omit_body` フィールドに RFC 9110 Section 9.3.2 / Section 6.4.1 の参照を追加し、1xx/204/304 はエンコーダーが自動抑止するため不要であることも明記する
  - @voluntas
```

注: 誤った RFC 節番号の修正は doc 上のバグ修正に該当するため `[FIX]` として独立エントリ化する。それ以外の doc 追加は `[UPDATE]` とする。AGENTS.md の種別順序 (UPDATE → ADD → CHANGE → FIX) に従い `[UPDATE]` を先に、`[FIX]` を後に記載する。

## 検証方針

- doc のみの変更なので機能的な検証は不要
- `cargo doc --no-deps` でドキュメントが警告なくビルドされることを確認。`HttpHead::is_keep_alive` 等のトレイトメソッドの doc が `cargo doc` 出力に正しく表示されることを目視確認する
- `///` を追加した全メソッドについて、`cargo doc` の出力に RFC 節番号が含まれていることを目視確認。`omit_body` フィールドは非公開で `//` コメントのため `cargo doc` に表示されないのでソースコード直接確認とする
- `make fmt && make clippy && make check && make test` が doc 変更前と同じく成功することを確認

## 受け入れ基準

- `make fmt && make clippy && make check && make test` がすべて成功する
- `cargo doc --no-deps` が警告なく完了する
- `src/response.rs` の以下 7 項目の doc に RFC 9110 または RFC 9112 の節番号 (Section X.Y.Z 形式) が含まれている: `omit_body` (フィールド)、`omit_body` (メソッド)、`is_body_omitted`、`is_keep_alive`、`is_chunked`、`content_length`、`connection`
- `src/request.rs` の以下 4 メソッドの doc に RFC 9110 または RFC 9112 の節番号 (Section X.Y.Z 形式) が含まれている: `is_keep_alive`、`is_chunked`、`content_length`、`connection`
- `src/decoder/head.rs` の以下 4 メソッドの doc に RFC 節番号が含まれている: `connection`、`content_length`、`is_keep_alive`（修正後）、`is_chunked`（既存、再確認のみ）
- `head.rs` の `is_keep_alive` doc に誤った `RFC 9110 Section 9.1` が残っていない
- `head.rs` の `is_keep_alive` doc に正しい節番号として `RFC 9112 Section 9.3`、`RFC 9112 Section 9.6`、`RFC 9110 Section 7.6.1`、`RFC 9110 Section 5.3` が含まれている
- `is_keep_alive` の doc に `Connection: close` の影響（HTTP/1.1 でも `close` 指定で keep-alive が無効になること、および `close` が `keep-alive` より優先されること）が明記されている
- `content_length` の doc に「最初のヘッダー値のみを返す」ことが明記されている
- `connection` の doc に「そのままの &str を返し、トークン分割は行わない」ことが明記されている
- `head.rs` の `connection` メソッド doc にもトークン分割方法 (`split(',')`) が記載されている
- `omit_body` フィールドの doc に「将来撤去される可能性がある」(pending/0018) が明記されている
- 構造体冒頭 doc (L18-20) の `omit_body` の記述に `RFC 9110 Section 6.4.1`、将来変更の可能性、1xx/204/304 は `omit_body` 不要であること、304 のヘッダー制約の特殊性 (Transfer-Encoding / Content-Length 設定は拒否されないが body 送出は抑止される) が追記されている
- `omit_body` メソッドの doc に `body` 非空時の Content-Length 自動付与の挙動、空ボディ (`Some(vec![])`) 時の Content-Length 自動付与抑止、1xx/204/304 では不要であることが明記されている
