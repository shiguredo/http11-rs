//! Content-Language ヘッダーのユニットテスト

use shiguredo_http11::content_language::ContentLanguage;

#[test]
fn parse_empty_elements() {
    let cl = ContentLanguage::parse("").unwrap();
    assert!(cl.tags().is_empty());

    let cl = ContentLanguage::parse(",").unwrap();
    assert!(cl.tags().is_empty());

    let cl = ContentLanguage::parse("en,,ja").unwrap();
    assert_eq!(cl.tags().len(), 2);
}

#[test]
fn display() {
    let cl = ContentLanguage::parse("en-US, ja").unwrap();
    assert_eq!(cl.to_string(), "en-US, ja");
}

#[test]
fn parse_primary_subtag_alpha_only() {
    // BCP 47/RFC 5646: 先頭サブタグは ALPHA のみ
    // 数字で始まる言語タグは不正
    assert!(ContentLanguage::parse("123").is_err());
    assert!(ContentLanguage::parse("1ab").is_err());
    // 後続サブタグは ALPHA / DIGIT OK
    let cl = ContentLanguage::parse("en-123").unwrap();
    assert_eq!(cl.tags()[0], "en-123");
}
