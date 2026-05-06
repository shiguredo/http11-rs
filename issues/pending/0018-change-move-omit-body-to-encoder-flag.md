# 0018: Response::omit_body を encoder 側のフラグへ移譲する

Created: 2026-05-06
Model: Opus 4.7

## pending 理由

`Response::omit_body` は `closed/0004-change-request-response-body-optional.md` で意図的に残された設計判断 (HEAD レスポンスで「Content-Length は表現長として残しつつ message body を送らない」を実現する直交フラグ)。本 issue は `omit_body` を `Response` 値オブジェクトから外し、encoder 側のフラグに移譲する提案で、`closed/0004` の決定を覆す方向の設計変更となる。

設計判断が必要なため pending とする。判断材料:

- HEAD レスポンスを生成する利用パターンを examples で具体化してから決定する
- HTTP セマンティクスとして「Response 値オブジェクトに送信制御を含めるか否か」の方針を確定する
- `closed/0004` の前提を覆す価値があるかをチームで合意する

## 概要

`Response::omit_body` フラグを `Response` 構造体から削除し、encoder 側の API として「ヘッダーのみ送信し本体を送らない」フラグを提供する形に再設計する。`Response` は「メッセージとして何を表現するか」だけを持ち、「どう送るか」は encoder の責務に純化する。

破壊的変更。`Response::omit_body(true)` を呼んでいる箇所はすべて encoder 呼び出しの形に書き換える。

## 根拠

### 問題 1: データ型に送信制御フラグが混じっている

`Response` は HTTP レスポンスの値オブジェクト (バージョン / ステータス / ヘッダー / ボディ) を表現するが、現状の `omit_body: bool` は「実際にバイト列として送信するときの挙動制御」であり、データ表現とは異なるレイヤーの関心事。値オブジェクトに送信制御が混ざることで、以下が起きる:

- 同じ `Response` 値を複数経路で送信する場合に、経路ごとに `omit_body` を切り替えたいケースがある
- HEAD と GET で同じ Response 構造体を共有したいケースで、`omit_body` を毎回切り替える必要がある
- データ型のシリアライズ・比較・クローン等が「送信意図」まで含むことになり、意味論が曖昧

### 問題 2: `omit_body` という否定形 bool が読みづらい

`omit_body: false` を読んで「省略しない (= 送信する)」と二重否定で理解する負担がある。慣習的には肯定形 (`send_body: true` がデフォルト) や enum (`BodyTransmission::Send | Skip`) の方が意図が明確。

### 問題 3: `body` と `omit_body` の組み合わせが直感的でない

「`body: Some(data)` で `Content-Length` を計算するが、実体は送らない」という HEAD レスポンスの挙動は、データ表現としては一貫しているが API 利用者には混乱を招く。`representation_length` のような明示的なフィールドを別途持たせる選択肢もある。

## 対応方針

### src/response.rs

- `omit_body: bool` フィールドを削除する
- `pub fn omit_body(self, omit: bool) -> Self` ビルダーメソッドを削除する
- 関連する doc コメントを整理する

### src/encoder.rs

HEAD レスポンスを送信するための新 API を提供する。具体案 (実装裁量):

#### 案 A: encode 関数のフラグ引数

```rust
pub fn encode_response(response: &Response, omit_body: bool) -> Result<Vec<u8>, EncodeError>
```

- 既存の `encode_response(&response)` から破壊的に変更
- 利用側は HEAD レスポンス送信時に `encode_response(&response, true)` を呼ぶ

#### 案 B: 専用 encode 関数

```rust
pub fn encode_response(response: &Response) -> Result<Vec<u8>, EncodeError>
pub fn encode_response_head_only(response: &Response) -> Result<Vec<u8>, EncodeError>
```

- 既存 API は維持
- HEAD 用に別関数を新設

#### 案 C: ResponseEncoder 等のステートフル encoder を導入する

将来の拡張性 (chunked エンコードのストリーミング等) を考慮するなら、ResponseEncoder 構造体を導入し設定で制御する。本 issue の範囲を超える可能性が高いので別 issue 化候補。

### examples / tests / pbt

- `.omit_body(true)` の呼び出しを上記新 API に書き換える
- HEAD レスポンスの単体テストが encoder の新 API を経由するように書き換える
- ラウンドトリップ PBT で「送信ではない」(omit_body) は `Response` の比較対象から外れる (構造体に存在しないため自動で解消)

## CHANGES.md

`## develop` に以下を追加する (本 issue が pending 解除されたら反映):

```
- [CHANGE] `Response::omit_body` を撤去し、HEAD レスポンスの送信制御は encoder 側 API に移譲する
  - `Response` は値オブジェクトとして「表現」のみを担い、「送信時の挙動制御」は encoder の責務に純化する
  - `.omit_body(true)` を呼んでいた箇所は新 encoder API に書き換える
  - @voluntas
```

## 検証方針

- HEAD レスポンス送信時に Content-Length は維持されるが本体バイトが出力されない、という挙動が新 API で保たれることを単体テストで検証する
- 既存の HEAD 系テストが新 API に追従して green になることを確認する

## 受け入れ基準

- `make fmt && make clippy && make check && make test` がすべて成功する
- `src/response.rs` から `omit_body` フィールドが消えている
- HEAD レスポンスの「ヘッダーのみ送信」挙動が新 API で再現できる単体テストが存在する
