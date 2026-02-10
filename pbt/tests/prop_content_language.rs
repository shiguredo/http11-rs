//! Content-Language ヘッダーのプロパティテスト (content_language.rs)

use proptest::prelude::*;
use shiguredo_http11::content_language::ContentLanguage;

// 言語タグ生成は pbt クレートを使用
use pbt::language_tag;

// Content-Language のラウンドトリップ
proptest! {
    #[test]
    fn prop_content_language_roundtrip(tags in proptest::collection::vec(language_tag(), 1..4)) {
        let header = tags.join(", ");
        let parsed = ContentLanguage::parse(&header).unwrap();
        let displayed = parsed.to_string();
        let reparsed = ContentLanguage::parse(&displayed).unwrap();
        prop_assert_eq!(parsed, reparsed);
    }
}
