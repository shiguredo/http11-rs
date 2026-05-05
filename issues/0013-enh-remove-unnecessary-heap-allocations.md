# 0013: 各所の不要なヒープ確保と計算量悪化を除去する

Created: 2026-05-05
Model: Opus 4.7

## 概要

`src/` を **ヒープ確保と計算量** の観点で精査し、堅牢性に直結する不要なヒープ確保と計算量悪化を除去する。CLAUDE.md の **「Premature Optimization is the Root of All Evil」** および **「性能より堅牢性を優先」** に従い、対象は以下の 3 カテゴリのみ:

- **計算量の悪化**: 項目 2 の `MultipartParser` バッファ再コピー（パート数 N に対し O(N²) のコピー量）
- **同一値の毎回再生成**: 項目 3 の boundary 文字列のように、構築時に確定する値を呼び出し毎に `format!` する箇所
- **頻度の高いアロケーション**: 項目 1 のチャンクストリーミング送信のように、1 メッセージあたり N 回発生する `format!`

容量予約の調整など「一般論として速くなる」だけの提案は原則対象外。項目 4 のみ「再確保時の一時メモリ重複の最大値を抑える」点で堅牢性に弱く寄与するため候補に残す（実装可否はベンチで判断）。

`alloc::format!` の他の使用箇所（`auth.rs`, `cache.rs`, `uri.rs` 等）はいずれも 1 メッセージあたりの呼び出し回数が定数（O(1)）で、上記 3 カテゴリのいずれにも該当しないため対象外。

いずれの変更も公開 API を変更せず、内部実装の改善に留まる（後方互換あり）。

## 着手項目（優先度順）

| 優先度 | 項目 | 対象ファイル | 概要 |
|---|---|---|---|
| 高 | 2 | `multipart.rs` | `MultipartParser` バッファコピーを O(N²) → amortized O(N) |
| 高 | 3 | `multipart.rs` | `MultipartParser` デリミタを `new()` で事前計算 |
| 中 | 1 | `encoder.rs` | `encode_chunk` / `encode_chunks` の hex 生成をスタックバッファ化 |
| 低 | 6 | `encoder.rs` | ステータスコード / Content-Length の `to_string()` をスタックバッファ化 |
| 低 | 4 | `encoder.rs` | `encode_request` / `encode_response` の `with_capacity` 化（ベンチ判定） |

非着手項目: 項目 5（デコーダー中間 String）、項目 7（CacheControl ビットフラグ）、項目 8（headers 初期容量）。判断記録は後述。

注記: 以下の本文セクションは **項目番号順**（1, 2, 3, 6, 4）で並べているが、これは関連項目（同じファイル / 同じヘルパーパターン）をグループ化して読みやすくするため。**着手すべき優先度は上記表の通り**（2, 3, 1, 6, 4）。

### 1. `encode_chunk` / `encode_chunks` — チャンクサイズ文字列で毎回ヒープ確保

`encoder.rs:750`, `767`:

```rust
buf.extend_from_slice(alloc::format!("{:x}\r\n", data.len()).as_bytes());
```

チャンク数が多いストリーミング送信で `alloc::format!` のヒープ確保が頻発する。

**修正方針**: スタックバッファ `[u8; 16]` 上で 16 進数を構築する `write_hex_usize` ヘルパーを `encoder.rs` 内のプライベート関数（`fn` のみ、`pub`/`pub(crate)` を付けない）として配置。CRLF はヘルパー外で結合する。

```rust
fn write_hex_usize(buf: &mut Vec<u8>, n: usize) {
    if n == 0 {
        buf.push(b'0');
        return;
    }
    let mut tmp = [0u8; 16]; // 64bit usize の 16 進表記は最大 16 桁
    let mut i = tmp.len();
    let mut remaining = n;
    while remaining > 0 {
        i -= 1;
        let nibble = (remaining & 0xF) as u8;
        tmp[i] = if nibble < 10 { b'0' + nibble } else { b'a' + nibble - 10 };
        remaining >>= 4;
    }
    buf.extend_from_slice(&tmp[i..]);
}
```

`n == 0` を早期分岐で扱うのは、`leading_zeros` で桁数を計算する変種が `n == 0` で誤桁数を返すのを避けるため。呼び出し側で `write_hex_usize(&mut buf, data.len()); buf.extend_from_slice(b"\r\n");` の形に分離する。

**併せて**`encode_chunk` / `encode_chunks` の `Vec::new()` を `Vec::with_capacity` に変更する。1 チャンクの出力構造は `hex(最大 16) + CRLF(2) + data + CRLF(2) = data.len() + 20` バイト、終端チャンクは `b"0\r\n\r\n" = 5` バイト。

- `encode_chunk` 非空: `data.len() + 20`
- `encode_chunk` 空: `5`（終端チャンク `b"0\r\n\r\n"`）
- `encode_chunks`: `chunks.iter().map(|c| c.len() + 20).sum::<usize>() + 5`（末尾の `+5` は終端チャンク `b"0\r\n\r\n"`）

**整数オーバーフロー対策**: 上記式の `+ 20` / `.sum()` / `+ 5` はいずれも `usize` 加算でオーバーフロー可能（攻撃者制御の `data` / `chunks` で `usize::MAX` 近傍を狙われると wraparound して過小確保になる）。実装では `checked_add` ベースで計算し、オーバーフロー時は `Vec::new()` にフォールバックするか、`saturating_add` で `usize::MAX` まで詰める。後者なら `Vec::with_capacity(usize::MAX)` の OOM panic に頼る形になるため、**前者（checked_add + フォールバック）を推奨**。

**テスト方針**: `pbt/tests/prop_encoder.rs` に PBT を追加し、任意 `usize` で `write_hex_usize` の出力が `alloc::format!("{:x}", n).as_bytes()` と完全一致することを検証する（既存実装との等価性）。併せて、出力文字列を `usize::from_str_radix(s, 16)` で復元すると元の値に戻るラウンドトリップ性も検証する（絶対的正しさの担保）。境界値 `0`, `1`, `15`, `16`, `255`, `256`, `usize::MAX` は単体テストで個別検証。

### 2. `MultipartParser::next_part()` — バッファ先頭除去で残り全体を毎回コピー

`multipart.rs:312`, `321`, `324`, `396`, `398`, `401`:

```rust
self.buffer = self.buffer[after_delim + 2..].to_vec();
```

`Vec<u8>` の先頭を `to_vec()` で切り詰めるたびに残り全体を別の `Vec` にコピーしている。パートが N 個あれば同じバイト列が N 回コピーされる（O(N²)）。fuzzing で多数パートを含む multipart ボディを投入されると、アロケーション連打とコピー量の増大により容易にメモリを圧迫する。

**修正方針**: `MultipartParser` に `pos: usize` フィールドを追加し、読み取り位置をオフセットで管理する。公開 API は変更しない（内部フィールド追加のみ）。

- 全バッファアクセスを `&self.buffer[self.pos..]` 経由にする。
- `find_bytes` の戻り値は `&self.buffer[self.pos..]` 内の **相対位置**。絶対オフセットへの変換は必ず `self.pos + rel_pos` を計算し、`body_start` / `body_end` / `after_delim` 等の導出すべてで漏らさない。
- 既存コードの `if let Some(pos) = find_bytes(...)` の `pos` は `self.pos` と同名でシャドーイングのバグ源。実装時は `rel_pos` 等にリネームする。
- パートのボディは `self.buffer[body_start..body_end].to_vec()` で 1 回だけコピー（`Part` 構造体への所有権移転で不可避）。
- 前詰めは **`next_part()` がパートを返す直前**の 1 箇所に集約し、`self.pos > self.buffer.len() / 2` のときだけ `self.buffer.drain(..self.pos); self.pos = 0;` で実行する。`Initial → InPart` 遷移時など途中の `pos` 更新では drain しない。
- `drain` は残要素の memmove を伴うが、上記発動条件により**累積コピー量は `O(total_size)`** に抑えられる。

**目標**: パート数 N に対し O(N²) になっていたコピー量を amortized O(N) に改善する。

**Debug 表示の注意**: `#[derive(Debug)]` のままだと `buffer` の先頭〜`pos` 部分が「消費済みだが未 drain」のデータとして見える。本 issue ではカスタム Debug 実装は行わず、`pos` を見て読む運用で対応する（Debug 誤読は自動テスト対象外。コードレビューで「`buffer` を直接参照していないか」を確認）。

**テスト方針**:

- 既存の `pbt/tests/prop_multipart.rs` および単体テストが全通過することを確認（公開 API 不変）。
- 単体テスト追加: `feed(部分データ) → next_part() = Incomplete → feed(残データ) → next_part() = Some(Part)` のシーケンス。境界をまたいだ feed で `pos` と `buffer` の整合性が保たれることを保証する。
- drain 発動の検証: 同一 boundary のパートを 10 個以上含むボディを feed し、`next_part()` を順に呼んで全パート取得。途中で drain が発動して `pos` がリセットされること、得られる全パートが期待値と一致することを検証する。`pos` は非公開フィールドのため、テストから観察するには **`#[cfg(test)] fn test_pos(&self) -> usize` のような内部アクセサを `multipart.rs` 内に追加**して経由する（`pub` にはしない）。アクセサ追加を避けたい場合は、`buffer.len()` の減少を観察する間接検証で代替する。

### 3. `MultipartParser::next_part()` — デリミタ文字列を毎回 `alloc::format!` で生成

`multipart.rs:301`, `384`:

```rust
let delimiter = alloc::format!("--{}", self.boundary);       // next_part() ごと
let next_delim = alloc::format!("\r\n--{}", self.boundary);   // パートごと
```

boundary はパーサー構築時に決まる固定値なのに、呼び出し毎に `format!` で再生成している。

**修正方針**: `new()` で `first_delimiter: Vec<u8>`（`b"--" + boundary`）と `inner_delimiter: Vec<u8>`（`b"\r\n--" + boundary`）を事前計算してフィールドに持つ。`find_bytes` はバイト列を取るため `Vec<u8>` で保持して `.as_bytes()` 呼び出しを省く。構築は `format!` を使わずバイト列結合で行う:

```rust
let mut first_delimiter = Vec::with_capacity(2 + boundary.len());
first_delimiter.extend_from_slice(b"--");
first_delimiter.extend_from_slice(boundary.as_bytes());

let mut inner_delimiter = Vec::with_capacity(4 + boundary.len());
inner_delimiter.extend_from_slice(b"\r\n--");
inner_delimiter.extend_from_slice(boundary.as_bytes());
```

`MultipartParser` の既存 `boundary: String` フィールドは公開アクセサがなく内部利用のみ（`MultipartBuilder` 側は無関係）。デリミタ事前計算後は冗長になるため**削除する**。

**命名について**: `first_delimiter` / `inner_delimiter` は意図が直感的だが、RFC 2046 Section 5.1.1 の用語（delimiter / close-delimiter 等）と厳密には一致しない。実装時に RFC 用語に寄せた命名（`opening_delimiter` / `separator_delimiter` 等）も検討する。本 issue では拘束しない。

### 6. `Content-Length` / ステータスコードの `to_string()` — 少数値で毎回ヒープ確保

`encoder.rs:556`, `653`, `686`:

```rust
buf.extend_from_slice(body.len().to_string().as_bytes());
buf.extend_from_slice(response.status_code.to_string().as_bytes());
```

**修正方針**: 項目 1 と同じスタイルで `write_usize_decimal` を**別関数**として `encoder.rs` のプライベート関数で用意する。

```rust
fn write_usize_decimal(buf: &mut Vec<u8>, n: usize) {
    if n == 0 {
        buf.push(b'0');
        return;
    }
    let mut tmp = [0u8; 20]; // 64bit usize の 10 進表記は最大 20 桁
    let mut i = tmp.len();
    let mut remaining = n;
    while remaining > 0 {
        i -= 1;
        tmp[i] = b'0' + (remaining % 10) as u8;
        remaining /= 10;
    }
    buf.extend_from_slice(&tmp[i..]);
}
```

ステータスコードは `u16`（最大 3 桁）だが、ヘルパーは `usize` に統一して使い回す（呼び出し側で `code as usize`）。本リポジトリは `usize >= 32bit` を前提とする標準的な Rust ターゲットのみをサポートするため、このキャストは無損失で安全。

**テスト方針**: 項目 1 と同様、PBT で `alloc::format!("{}", n).as_bytes()` との等価性を検証。併せて出力を `s.parse::<usize>()` で復元すると元の値に戻るラウンドトリップ性も検証する。境界値 `0`, `9`, `10`, `99`, `100`, `usize::MAX` は単体テスト。

**位置付け**: 1 メッセージあたりの効果は数バイト × 数アロケーション程度で極めて小さい。これ単独では着手せず、**項目 1 完了後に同パターンで適用**する。項目 6 と項目 4 はスコープが異なる（項目 6 は `to_string()` のみ、項目 4 は `Vec::new()` の置換）。項目 6 完了後も項目 4 を見送れば `Vec::new()` は残るが、混在状態は許容する。

### 4. `encode_request` / `encode_response` — バッファが `Vec::new()` で再確保を繰り返す

`encoder.rs:530`, `648`:

```rust
let mut buf = Vec::new();
```

ボディ長が事前にわかっているのに初期容量 0 のため `extend_from_slice` を繰り返す過程で Vec が段階的に再確保される。再確保時は旧バッファと新バッファが一時的に同時に存在するため、**ボディが大きいほど一時メモリ重複量が増える**（堅牢性側の懸念）。再確保回数自体（O(log N)）は副次的問題。

なお、同じ `Vec::new()` パターンの `encode_chunk` / `encode_chunks` は項目 1 で対処する（容量計算が単純）。本項目はリクエスト/レスポンス全体エンコードのみを対象とする。

**修正方針**: 容量計算用にヘッダーをもう 1 周走らせて合計サイズを求め `Vec::with_capacity` する。書き込みループとあわせて全体ではヘッダーを 2 周する。

容量計算の構成要素:

- リクエストライン (`method + SP + uri + SP + version + CRLF`): `method.len() + uri.len() + version.len() + 4`
- ステータスライン (`version + SP + status_code + SP + reason + CRLF`): `version.len() + reason.len() + 3 + 4`
  - ステータスコード桁数は**固定値 3**（u16 で取り得る最大桁数。最大 2 バイトの過剰確保は許容）
- 全ヘッダー: `headers.iter().map(|(n, v)| n.len() + v.len() + 4).sum()`（`": "` と `"\r\n"` で 4 バイト）
- Content-Length 自動付与分: **固定値 38 バイト**を加算（`"Content-Length: " (16) + usize 最大 20 桁 + "\r\n" (2) = 38`）
- end-of-headers `"\r\n"` (2 バイト)
- ボディ長

Content-Length 桁数の厳密計算はしない（二度走査回避と過剰確保の両立）。

**整数オーバーフロー対策（必須）**: 容量計算は攻撃者制御の入力（巨大ヘッダー / 巨大ボディ）を含む `usize` 加算の連続。素朴な `+` で書くと wraparound で過小確保となり、再確保連発で最適化の意味を失う。実装では **`checked_add` で全加算を検証**し、いずれかでオーバーフローが起きた場合は **`Vec::new()` にフォールバック**する（既存挙動と同等で安全側に倒す）。`saturating_add` で `usize::MAX` まで詰めるアプローチは次項の OOM panic に直結するため避ける。

**OOM サニティチェック（必須）**: `Vec::with_capacity(huge)` は内部の `alloc` が fallible でないため、巨大値で abort または panic する。攻撃者制御のヘッダー値（例: reflected-input）で見積もりが膨らむと、**プロセス全体が abort する DoS** を引き起こす。容量が一定上限（例: `64 MB` 程度を `const ENCODE_CAPACITY_LIMIT: usize` で定義）を超える場合は `Vec::new()` にフォールバックすること。上限値は実装時にベンチで確認し、現実的な HTTP メッセージサイズを十分カバーする値にする。

  なお、本来は `encode_*` の入力サイズは呼び出し側（サーバー実装の上限制御）でガードされる前提だが、**ライブラリ側で防御線を張る方が堅牢**（CLAUDE.md「性能より堅牢性を優先」と整合）。

**実装上のリスク**: 容量計算と書き込みで分岐条件式（Content-Length 自動付与判定など）が二重になる。条件がずれると過小確保 → 再確保で意味を失う。判定ロジックは単一関数に括り出し、両側から呼び出すこと。`encode_request` と `encode_response` で判定ロジックの形が異なるため、それぞれ別関数として括り出す（例: `should_auto_emit_content_length_for_request` / `..._for_response`）。

**fuzz ターゲット新設（必須）**: 容量計算ロジックは攻撃面が大きく、本項目を実装するなら **`fuzz_encode_request` / `fuzz_encode_response` を新設すること**を必須条件とする。最低限以下を検証:

- 任意の `Request` / `Response` を入力にしても `encode_*` が panic / abort しない
- オーバーフロー対策のフォールバックパスでも出力が正しい
- 容量見積もりが実際の出力長以上である（過小確保が発生しないこと）

fuzz ターゲットなしで容量計算を入れるのは堅牢性観点でリスクが高いため、**fuzz の新設をスキップしての項目 4 単体実装は禁止**。

**着手判断**: 堅牢性寄与は限定的なため**優先度は最低**。実装前に再確保回数を計測して PR 説明に記載し、誤差レベルなら見送る。

## 判断記録（非着手項目）

### 5. デコーダーのヘッダー行パース — 中間 `String` 確保

`decoder/request.rs:307`, `465`、`decoder/response.rs:425`, `557`: ヘッダー行あたり 3 回の `String` アロケーション（`line` 中間 String + `name` + `value`）。name/value はヘッダー保持で不可避なため、削減できるのは中間 `line` の 1 個のみ。

**着手しない理由**:

- `parse_header_line` をバイト列受け取りに変更すると影響範囲が広く、ABNF バリデーションの再実装が必要。費用対効果が悪い。
- 「性能より堅牢性を優先」に照らすと、バイト列パスでバリデーションを再実装する方がバグ混入リスクが高い。

**今後**: 現時点で `issues/pending/` への新規 issue 作成は行わない。fuzz/プロファイルで実害が観測された時点、または observability 向上後に再評価する。

### 7. `CacheControl` — bool フィールドが 10 個

`cache.rs:74-92`: 10 個の `bool` を `u16` ビットフラグ化すれば 10 バイト → 2 バイト。

**着手しない理由**:

- `CacheControl` は通常 1 リクエスト/レスポンスあたり 1 個生成して短命に消える使い方で、大量保持シナリオが本リポジトリに存在しない。
- 10 バイト → 2 バイトの削減はメモリ効率上の意味がほぼ無い。
- ビットフラグ化は実装の可読性が下がり、ビット位置の管理ミスでサイレントにバグが入る余地が生まれる。

### 8. `headers: Vec::new()` — 初期容量指定なし

`request.rs:42`, `response.rs:49`, `decoder/head.rs:117`, `140`: 容量 0 から開始するため `4 → 8 → 16` の再確保が発生する。

**着手しない理由**:

- `(String, String)` のサイズは 64bit 環境で 48 バイト。`with_capacity(32)` は 1 メッセージあたり 1.5KB を必ず確保。最小構成のリクエストでも同じ容量を取る。
- 高同時接続環境や組み込み用途では過剰確保がメモリ圧迫の原因になりうる（堅牢性を悪化させる方向）。
- 「典型値 32」の根拠が示せない（テストコーパスや実運用ログによる典型ヘッダー数の実測値ではない）。

**今後**: 仮に行うとしても、まずヘッダー数の分布を実測してから容量を決める別 issue とする。

## 運用

### PR とコミット

- **PR は項目単位で出す**。レビュー・revert・進捗管理を簡潔にするため。
- ただし **同じファイル内で関連が深い項目は 1 ブランチ・1 コミット・1 PR にまとめる**:
  - **項目 2 + 項目 3**: 両方 `MultipartParser` の同じ構造体を触り `pos` 導入と `boundary`/デリミタ整理が相互依存的。
  - **項目 1 + 項目 6**: 同じヘルパーパターンの適用（`write_hex_usize` と `write_usize_decimal`）。
- ブランチ命名は `feature/add-` を使う（CLAUDE.md の命名規則に「内部リファクタ」カテゴリが無いため。命名規則自体の整理は別 issue 化を推奨）。

### 共通チェックリスト

公開 API は変更しないため、以下は全項目で必須:

- 単体テスト全実行: `cargo test -p shiguredo_http11`
- PBT 全実行: `cargo test -p pbt`
- Fuzzing 短時間実行（30 秒以上、対象 fuzz ターゲットが存在する場合のみ）:
  - 項目 2, 3（multipart）: `cargo fuzz run fuzz_multipart`
  - 項目 1（chunk 系）: `cargo fuzz run fuzz_decoder_chunked`
  - 項目 6（status code / Content-Length）: `write_usize_decimal` の使用箇所（`encode_request` / `encode_response`）に対する fuzz ターゲットは存在しない。**ヘルパー単体は PBT で `format!` との等価性 + ラウンドトリップ検証**で担保し、fuzzing は不要とする
  - 項目 4（`encode_request` / `encode_response`）: **項目 4 を実装する場合は `fuzz_encode_request` / `fuzz_encode_response` を新設して実行**（必須。詳細は項目 4 を参照）
- llvm-cov でカバレッジ取得・後退がないことを確認: `cargo llvm-cov`
- ビルド警告ゼロを確認: `cargo build` および `cargo clippy -- -D warnings`（項目 6 完了後は `encoder.rs` の `use alloc::string::{String, ToString};` から `ToString` が未使用になる可能性がある。`unused_imports` は rustc 標準 lint なので `cargo build` で検出される。`ToString` のみが未使用になる場合は `use alloc::string::String;` に縮める。**`String` は引き続き使われていることを確認**してから縮めること（両方削除するとコンパイルエラー）。）
- 変更箇所の追加テストを書く（ヘルパーは PBT で `format!` との等価性 + ラウンドトリップを検証）

### CHANGES.md エントリ

`## develop` 配下に **`### misc` サブセクション**を作り、項目ごとに `[UPDATE]` で 1 行ずつ追記する（外部から観測可能な機能変更ではないため）。

```markdown
### misc

- [UPDATE] `MultipartParser` のバッファ管理を読み取り位置オフセット方式に変更する
  - 多数パートの multipart ボディに対するコピー量を `O(N²)` から amortized `O(N)` に改善する
  - boundary 文字列のデリミタを `MultipartParser::new()` で事前計算してフィールドに持ち、`next_part()` ごとの `format!` を除去する
  - @voluntas
- [UPDATE] `encode_chunk` / `encode_chunks` のチャンクサイズ生成からヒープ確保を除去する
  - 16 進数文字列の生成にスタックバッファを使う `write_hex_usize` ヘルパーを導入し、ストリーミング送信時の `format!` を除去する
  - 併せてステータスコード / Content-Length の `to_string()` を `write_usize_decimal` ヘルパーに置き換える
  - @voluntas
```

項目 4 を実装する場合は別エントリを追加。

```markdown
- [UPDATE] `encode_request` / `encode_response` のバッファに `Vec::with_capacity` を導入する
  - 再確保時の一時的なメモリ重複を抑える
  - @voluntas
```
