//! PBT テスト共通ユーティリティ

use proptest::prelude::*;

// ========================================
// 言語タグ生成 (BCP 47/RFC 5646)
// ========================================

/// 先頭サブタグ: ALPHA のみ (1-8 文字)
pub fn language_primary_subtag() -> impl Strategy<Value = String> {
    "[A-Za-z]{1,8}".prop_map(|s| s)
}

/// 後続サブタグ: ALPHA / DIGIT (1-8 文字)
pub fn language_subsequent_subtag() -> impl Strategy<Value = String> {
    "[A-Za-z0-9]{1,8}".prop_map(|s| s)
}

/// 言語タグ: primary-subtag *("-" subtag)
pub fn language_tag() -> impl Strategy<Value = String> {
    (
        language_primary_subtag(),
        proptest::collection::vec(language_subsequent_subtag(), 0..=2),
    )
        .prop_map(|(primary, rest)| {
            if rest.is_empty() {
                primary
            } else {
                format!("{}-{}", primary, rest.join("-"))
            }
        })
}
