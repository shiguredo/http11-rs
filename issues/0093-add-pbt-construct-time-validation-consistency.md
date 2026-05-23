# 構築時検査の PBT 整合性検証を整備する

- Priority: Medium
- Created: 2026-05-23
- Model: Opus 4.7

## 目的

issue 0091 で導入する `HeaderName` / `Method` / `Scheme` の構築時検査について、
以下の不変性を PBT (proptest) で恒常的に検証する。

1. **完全性**: `new() -> Result` が `Ok` を返す入力集合と、decoder が受理する入力集合が一致
2. **健全性**: `new()` が `Ok` を返した値は encoder → decoder の往復で同値を返す
3. **`from_static` と `new` の一貫性**: `const fn from_static` の検査ロジックと
   ランタイム検査 `new` が同じ判定をする
4. **`from_validated_parts` と `new` の整合性**: 内部用コンストラクタが公開 API と
   同じ結果を返す

## 優先度根拠

Medium。0091 と同じ。複数経路に分散する検査ロジック (`new` / `from_static` / decoder /
`from_validated_parts`) の整合性は、PBT で恒常的に検証しないとサイレントに崩れる
リスクが高い。0091 完了直後に着手すべき。

## 現状

http11 の既存 PBT (`pbt/tests/`) はメッセージ全体のラウンドトリップ
(`prop_request.rs` / `prop_response.rs` 等) はあるが、個別フィールド型の
構築点と decoder の受理集合の一致を検証する仕組みはまだない。

issue 0091 で型を新設するため、その型ごとに整合性プロパティを揃える。

## 設計方針

### 戦略 (Strategy) 集約

`pbt/src/lib.rs` (既存) に各構築時検査型の `valid_*` / `invalid_*` 戦略を追加する。

```rust
// pbt/src/lib.rs
pub mod strategies {
    use proptest::prelude::*;

    /// RFC 9110 §5.1: field-name = token
    /// token = 1*tchar, tchar = "!" / "#" / ... / DIGIT / ALPHA
    /// HTTP/1.1 では大文字小文字を許容
    pub fn valid_header_name() -> impl Strategy<Value = Vec<u8>> { ... }

    pub fn invalid_header_name_with_crlf() -> impl Strategy<Value = Vec<u8>> { ... }

    /// RFC 9110 §9.1: method = token、慣習上大文字
    pub fn valid_method() -> impl Strategy<Value = Vec<u8>> { ... }

    /// RFC 3986 §3.1: scheme = ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )
    pub fn valid_scheme() -> impl Strategy<Value = Vec<u8>> { ... }
}
```

### プロパティ実装

```rust
// pbt/tests/prop_header_name.rs (新規)
use proptest::prelude::*;
use shiguredo_http11::HeaderName;
use pbt::strategies::*;

proptest! {
    #[test]
    fn from_static_matches_new(name in valid_header_name()) {
        // from_static は &'static [u8] を要求するため Box::leak で擬似的に静的化
        let leaked: &'static [u8] = Box::leak(name.clone().into_boxed_slice());
        let via_new = HeaderName::new(&name).unwrap();
        let via_static = HeaderName::from_static(leaked);
        prop_assert_eq!(via_new.as_bytes(), via_static.as_bytes());
    }

    #[test]
    fn validated_parts_matches_new(name in valid_header_name()) {
        let via_new = HeaderName::new(&name).unwrap();
        let via_validated = HeaderName::from_validated_parts(
            std::borrow::Cow::Owned(name)
        );
        prop_assert_eq!(via_new, via_validated);
    }

    #[test]
    fn new_accepts_iff_decoder_accepts(bytes in proptest::collection::vec(any::<u8>(), 0..256)) {
        let via_new = HeaderName::new(&bytes).is_ok();
        // 同じバイト列を decoder 経路に通して受理されるか確認
        let via_decoder = /* RequestDecoder にヘッダーとして食わせる */;
        prop_assert_eq!(via_new, via_decoder);
    }
}

// pbt/tests/prop_method.rs / pbt/tests/prop_scheme.rs も同様
```

### 既存 PBT との統合

既存の `prop_request.rs` / `prop_response.rs` も、新型 (`HeaderName` / `Method`) を
使った構築に書き換える。これにより「構築時検査つきの型で組み立てたメッセージ全体が
encoder → decoder で同値」を引き続き担保する。

### 注意点

- `Box::leak` を使うとプロセスのメモリを徐々に消費する。proptest の 256 ケース × 数プロパティ
  程度では問題にならないが、CI で大量ケース化する場合は `#[ignore]` 付きの special run に分ける
  (CLAUDE.md は `#[ignore]` 禁止なので、leak しない代替パターンとして
  `unsafe { core::mem::transmute }` で寿命を伸ばす方法もあるが、本筋ではない)
- 代替: `from_static` 専用のプロパティは「`new` で成功した既知の小さな入力集合」に対する
  単体テストで担保し、PBT は `new` ↔ decoder の整合性に絞る選択肢もある

## 完了条件

- `pbt/src/lib.rs` に `valid_header_name` / `valid_method` / `valid_scheme` および対応する
  `invalid_*` 戦略が定義されている
- `pbt/tests/prop_header_name.rs` / `prop_method.rs` / `prop_scheme.rs` が新設され、
  以下のプロパティが実装されている:
  - `from_static` と `new` の結果一致
  - `from_validated_parts` と `new` の結果一致
  - `new` と decoder の受理集合一致
- 既存 `prop_request.rs` / `prop_response.rs` が新型を使う形に書き換えられている
- 既存の全テスト・PBT・fuzz が通る
- `CHANGES.md` の `## develop` の `### misc` に `[ADD]` エントリが追加されている

## 解決方法

実装順:

1. `pbt/src/lib.rs` に戦略モジュール `strategies` を追加
2. `pbt/tests/prop_header_name.rs` 新設、3 種プロパティ実装
3. `pbt/tests/prop_method.rs` 新設、同上
4. `pbt/tests/prop_scheme.rs` 新設、同上
5. 既存 `prop_request.rs` / `prop_response.rs` の入力組み立てを新型に追従
6. `CHANGES.md` に追記

## 関連

- 0091 (HeaderName / Method / Scheme 導入) に依存
