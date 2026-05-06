# 0023: Response の RFC 根拠と委譲メソッドの doc を補強する

Created: 2026-05-06
Model: Opus 4.7

## 概要

`Response` および `HttpHead` 経由の委譲メソッド群について、RFC の節番号引用、戻り値の意味論、複数値ヘッダーの扱いを doc コメントに明記する。CLAUDE.md「資料由来の機能を実装する場合は、根拠資料名、節番号、将来変更される可能性をコードコメントで明記すること」に追従する。

破壊的変更なし。doc のみの改善。

## 根拠

### 問題 1: RFC 節番号が記載されていない

CLAUDE.md「資料由来の機能を実装する場合は、根拠資料名、節番号、将来変更される可能性をコードコメントで明記すること」のルールに対し、`src/response.rs` の以下が違反している:

- `omit_body` フィールドの doc (HEAD レスポンスに RFC 9110 Section 9.3.2 / RFC 9112 Section 6.4.1 への参照なし)
- `omit_body` メソッドの doc (同上)
- `is_keep_alive` の doc (RFC 9110 への言及はあるが節番号なし)
- `is_chunked` の doc (RFC 9112 への言及はあるが節番号なし)

### 問題 2: 戻り値の意味論が doc 不在

```rust
pub fn content_length(&self) -> Option<u64>
pub fn connection(&self) -> Option<&str>
```

これらのメソッドについて、以下の挙動が doc に書かれていない:

- 複数の `Content-Length` ヘッダーが存在する場合の挙動 (一致なら採用 / 不一致ならエラー / 最初を返す等)
- `Connection` ヘッダーはカンマ区切りトークンリスト (RFC 9110 Section 7.6.1) だが、`&str` で返すのはヘッダー値全体か、最初のトークンか
- 各メソッドの委譲先 (`HttpHead`) の挙動とコメントが乖離する可能性

### 問題 3: `is_keep_alive` の doc が不正確

```rust
/// HTTP/1.1 ではデフォルトでキープアライブ
/// HTTP/1.0 では Connection: keep-alive が必要
```

実態としては `Connection: close` が指定されると HTTP/1.1 でも keep-alive にならないが、その点が doc に明記されていない。利用者が doc だけ読んで判断すると挙動を誤解する。

### 問題 4: `is_chunked` の doc が委譲先依存

委譲先 `HttpHead::is_chunked` の挙動とコメントが乖離するリスクがある。doctest で挙動を固定するか、委譲先の doc にリンクする。

## 対応方針

### src/response.rs

#### `omit_body` フィールドの doc 補強

```rust
/// ボディ送信を抑止するフラグ (HEAD レスポンス用)
///
/// HEAD レスポンスではヘッダーのみ送信し、メッセージボディを送信しない
/// (RFC 9110 Section 9.3.2 / RFC 9112 Section 6.4.1)。
/// `Content-Length` ヘッダーは表現長として残す。
///
/// 注: 0018 で encoder 側のフラグへの移譲が検討されており、本フィールドは
/// 将来撤去される可能性がある。
pub omit_body: bool,
```

#### `omit_body` メソッドの doc 補強

`pub fn omit_body(mut self, omit: bool) -> Self` のコメントに RFC 9110 Section 9.3.2 を引用する。

#### `is_keep_alive` の doc 修正

```rust
/// キープアライブ接続かどうかを判定 (RFC 9110 Section 9.3 / Section 7.6.1)
///
/// - HTTP/1.1: `Connection: close` が **指定されていない** 場合に keep-alive
/// - HTTP/1.0: `Connection: keep-alive` が **指定されている** 場合に keep-alive
///
/// `Connection` ヘッダーはカンマ区切りトークンリストとして扱う
/// (RFC 9110 Section 7.6.1)。
pub fn is_keep_alive(&self) -> bool { ... }
```

#### `is_chunked` の doc 修正

```rust
/// Transfer-Encoding が chunked かどうかを判定 (RFC 9112 Section 6.1)
///
/// `Transfer-Encoding` リストの最後が `chunked` かどうかを確認する。
/// 複数の `Transfer-Encoding` ヘッダーがある場合は連結して 1 つのリストとして扱う。
pub fn is_chunked(&self) -> bool { ... }
```

#### `content_length` の doc 補強

```rust
/// `Content-Length` ヘッダーの値を取得 (RFC 9110 Section 8.6)
///
/// 複数の `Content-Length` ヘッダーがある場合の扱い:
/// (委譲先 `HttpHead::content_length` の挙動に従う)
/// - 同じ値の重複 → 採用
/// - 異なる値の重複 → ?
/// - 値がパース不能 → `None`
///
/// 注: 厳密な挙動は decoder のバリデーションロジックに依存する。
/// 本メソッドはパース後の値を読むだけなので、不正な複数値ヘッダーは
/// decoder 側で `Err` として弾かれているはずである。
pub fn content_length(&self) -> Option<u64> { ... }
```

注: 実際の挙動は委譲先のコードを確認した上で正確に記載する。

#### `connection` の doc 補強

```rust
/// `Connection` ヘッダーの値を取得 (RFC 9110 Section 7.6.1)
///
/// `Connection` はカンマ区切りトークンリストだが、本メソッドは
/// ヘッダー値全体 (raw) を返す。トークンごとに判定したい場合は
/// `connection().map(|v| v.split(','))` 等の処理が必要。
///
/// (実装の詳細は委譲先 `HttpHead::connection` を参照)
pub fn connection(&self) -> Option<&str> { ... }
```

注: 実際の戻り値仕様は委譲先のコードを確認した上で正確に記載する。

### src/request.rs

`Request` 側にも同様の委譲メソッドが存在するため、同じ範囲の doc 補強を行う (本 issue で同時対応するか別 issue にするかは実装裁量)。

### src/decoder/head.rs (HttpHead トレイト)

委譲元の doc が委譲先と乖離しないよう、`HttpHead` トレイトの各メソッドにも RFC 節番号を含む doc を整備する。`Response` / `Request` 側はトレイトの doc にリンクする形に書き換える選択肢もある。

### doctest の追加 (任意)

挙動を固定するため、doctest を追加する選択肢:

```rust
/// # Examples
///
/// ```
/// use shiguredo_http11::Response;
///
/// let response = Response::new(200, "OK")
///     .header("Connection", "close");
/// assert!(!response.is_keep_alive());
/// ```
```

ただし `0017` でコンストラクタが `Result` を返すように変わるため、doctest の書き方は新 API に追従する必要がある。

## CHANGES.md

`## develop` セクションの `### misc` サブセクションに以下を追加する:

```
- [UPDATE] `Response` の RFC 根拠と委譲メソッドの doc を補強する
  - `omit_body` / `is_keep_alive` / `is_chunked` / `content_length` / `connection` に RFC 節番号を引用する
  - 複数値ヘッダーの扱いを doc に明記する
  - @voluntas
```

## 検証方針

- doc のみの変更なので機能的な検証は不要
- `cargo doc` でドキュメントが警告なくビルドされることを確認
- doctest を追加した場合は `cargo test --doc` で実行されることを確認

## 受け入れ基準

- `make fmt && make clippy && make check && make test` がすべて成功する
- `cargo doc --no-deps` が警告なく完了する
- 上記 5 メソッド (`omit_body`, `is_keep_alive`, `is_chunked`, `content_length`, `connection`) の doc に RFC 9110 / 9112 の節番号が含まれている
- `is_keep_alive` の doc に `Connection: close` の影響が明記されている
