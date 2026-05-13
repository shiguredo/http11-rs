# 0019: HttpVersion を enum 化する

Created: 2026-05-06
Completed: 2026-05-06
Model: Opus 4.7

## 概要

`Response::version` (および対応する `Request::version`) の型を `String` から enum (`HttpVersion`) に変更する。`Response::new` は HTTP/1.1 を暗黙のデフォルトにし、`with_version` は enum を引数に取るシグネチャに変更する。

破壊的変更。`String` を直接代入していた箇所、および `with_version("HTTP/1.0", ...)` のような文字列リテラル渡しはすべて `HttpVersion::V1_0` 等の enum 値に書き換える。

## 根拠

### 問題 1: 不正値が型レベルで防げない

`version: String` は `"HTTP/2.0"` / `"HTTP/9.9"` / `"garbage"` のような不正値を受け入れる。http11-rs は HTTP/1 系専用のクレートなので、HTTP/2 以降をデータ型レベルで拒否すべき。現状は encoder 実行時のバリデーションに依存しており、型システムの利点を活かせていない。

### 問題 2: 毎回ヒープ確保が走る

`Response::new` の内部で `version: "HTTP/1.1".to_string()` が実行され、呼び出すたびに `String` がヒープに確保される。`HttpVersion::V1_1` のような enum なら 1 バイトで済み、ヒープ確保不要。

### 問題 3: `with_version` の存在意義が不明確

`pub fn with_version(version: &str, ...)` の doc は「カスタムバージョンでレスポンスを作成」とのみ記載されており、何向けの API か不明。HTTP/1.0 サポート用なら明示すべきだし、HTTP/0.9 も視野なら enum 化すれば自明になる。

### 問題 4: 比較が文字列比較になっている

`is_keep_alive` 等の判定で `version == "HTTP/1.1"` のような文字列比較が行われる可能性がある (HttpHead 経由)。enum なら 1 命令で比較でき、case-sensitivity や前後空白等の罠もない。

## 対応方針

### src/lib.rs (もしくは新設の src/version.rs)

```rust
/// HTTP プロトコルバージョン
///
/// RFC 9112 Section 2.3: HTTP-version = HTTP-name "/" DIGIT "." DIGIT
/// 本クレートは HTTP/0.9, HTTP/1.0, HTTP/1.1 のみをサポートする
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HttpVersion {
    /// HTTP/0.9 (RFC は存在しない歴史的バージョン)
    V0_9,
    /// HTTP/1.0 (RFC 1945)
    V1_0,
    /// HTTP/1.1 (RFC 9112)
    V1_1,
}

impl HttpVersion {
    /// `"HTTP/1.1"` 形式の文字列スライスとして取得 (encoder 用)
    pub fn as_str(&self) -> &'static str { ... }

    /// `"HTTP/1.1"` 形式の文字列スライスから enum へパース (decoder 用)
    pub fn parse(s: &str) -> Result<Self, Error> { ... }
}
```

`Display` 実装も提供する。

### src/response.rs

- `version: String` を `version: HttpVersion` に変更
- `Response::new(status_code, reason_phrase)` の内部初期化を `HttpVersion::V1_1` に変更 (ヒープ確保ゼロ)
- `Response::with_version` のシグネチャを `with_version(version: HttpVersion, status_code: u16, reason_phrase: &str)` に変更
- `HttpHead::version` の戻り値を `&str` から `HttpVersion` (または `&'static str`) に変更
  - 既存呼び出し側との互換性のため `&str` を維持し、内部で `as_str()` を呼ぶ選択もある

### src/request.rs

`Request::version` も同様に enum 化する。Request と Response は同時に対応する方が整合性が取りやすい。

### src/decoder/

- `start-line` パース時に `HttpVersion::parse` を呼ぶ
- 既存の `is_valid_protocol_version` バリデーションは `HttpVersion::parse` に集約する

### src/encoder.rs

- `version` の出力時に `HttpVersion::as_str()` を使う
- ヒープ確保削減により encoder のホットパスが軽くなる

### tests / pbt / examples

- `version: "HTTP/1.1".to_string()` のような直接代入を `HttpVersion::V1_1` に置換
- `with_version("HTTP/1.0", ...)` を `with_version(HttpVersion::V1_0, ...)` に置換
- ラウンドトリップ PBT は enum 化で簡素化される
- HTTP/2 等の不正バージョンを生成して decoder が `Err` を返すテストを追加

## CHANGES.md

`## develop` に以下を追加する:

```
- [CHANGE] `Request::version` / `Response::version` を `HttpVersion` enum に変更する
  - `HttpVersion::V0_9` / `V1_0` / `V1_1` の 3 バリアントをサポートする
  - `with_version` は enum を引数に取るシグネチャに変更する
  - HTTP/2 以降を型レベルで拒否し、ホットパスのヒープ確保を削減する
  - @voluntas
```

## 検証方針

### 既存挙動の保存

- 既存の単体テスト・PBT・examples が新 API に追従して green になる
- ラウンドトリップ PBT (`prop_response_roundtrip`, `prop_request_roundtrip`) で `HttpVersion::V1_0` / `V1_1` の両方が生成・パースされる

### 不正バージョン拒否

- decoder で `HTTP/2.0` を含むメッセージが `Err` を返すことを単体テストで検証
- decoder で `HTTP/garbage` のような形式違反が `Err` を返すことを PBT で検証

### HTTP/0.9 サポートの検討

HTTP/0.9 を実装するか、HTTP/1.0 と HTTP/1.1 のみとするかは要決定。RFC 1945 / 9112 の範囲で考えるなら HTTP/1.0 と HTTP/1.1 で十分という選択もありうる (HTTP/0.9 は status-line を持たないため、本クレートの構造体表現には収まりにくい)。

## 受け入れ基準

- `make fmt && make clippy && make check && make test` がすべて成功する
- `src/response.rs` / `src/request.rs` の `version` フィールドが `HttpVersion` 型になっている
- `HttpVersion::parse` で不正な文字列が `Err` になるテストが存在する
- ラウンドトリップ PBT が enum ベースで動作する

## クローズ理由

本 issue は却下し、対応しないことを決定した。理由は以下:

1. **RTSP サポートとの衝突**: 本クレートは HTTP/1.1 スタイルのメッセージ形式を前提としつつ、RTSP (RFC 7826) でも利用できる設計である。enum を `V0_9` / `V1_0` / `V1_1` に限定すると `RTSP/1.0` を型レベルで拒否してしまい、RTSP のユースケースを満たせなくなる。RTSP バリアントを追加しても、将来的な別プロトコル対応のたびに enum を拡張しなければならず、`String` の持つ汎用性を損なう

2. **`is_valid_protocol_version()` で十分に検証できている**: `src/validate.rs` の `is_valid_protocol_version()` は `token "/" DIGIT+ "." DIGIT+` 形式を検証しており、`encode` 時に不正なバージョン文字列は `EncodeError::InvalidVersion` として適切に弾かれる。型レベルでの防御は必須ではない

3. **パフォーマンス改善効果が小さい**: `new()` でのヒープ確保や `version != "HTTP/1.1"` の文字列比較は、encode/decode 全体のコストから見て無視できる規模であり、enum 化の複雑性を導入するだけの価値がない
