//! ETag ヘッダーのユニットテスト

use shiguredo_http11::etag::{ETagList, EntityTag, parse_etag_list};

#[test]
fn test_parse_weak_lowercase_rejected() {
    // RFC 9110 Section 8.8.3: weak = %s"W/" (case-sensitive)
    // 小文字 w/ は許可されない
    assert!(EntityTag::parse("w/\"abc123\"").is_err());
}

#[test]
fn test_parse_trailing_content_rejected() {
    // 閉じ引用符の後に余剰文字がある場合は拒否
    assert!(EntityTag::parse("\"abc\" extra").is_err());
    assert!(EntityTag::parse("W/\"abc\"extra").is_err());
}

#[test]
fn test_parse_missing_quote() {
    assert!(EntityTag::parse("abc").is_err());
    assert!(EntityTag::parse("\"abc").is_err());
    assert!(EntityTag::parse("abc\"").is_err());
}

#[test]
fn test_parse_empty() {
    assert!(EntityTag::parse("").is_err());
}

#[test]
fn test_parse_etag_list_any() {
    let list = parse_etag_list("*").expect("ETag のパースは成功するはず (実装バグ)");
    assert!(list.is_any());
}

#[test]
fn test_etag_list_contains() {
    let list = parse_etag_list("\"a\", W/\"b\"").expect("ETag のパースは成功するはず (実装バグ)");
    let etag_a = EntityTag::strong("a").expect("ETag のパースは成功するはず (実装バグ)");
    let etag_b = EntityTag::strong("b").expect("ETag のパースは成功するはず (実装バグ)");
    let etag_c = EntityTag::strong("c").expect("ETag のパースは成功するはず (実装バグ)");

    assert!(list.contains_weak(&etag_a));
    assert!(list.contains_weak(&etag_b));
    assert!(!list.contains_weak(&etag_c));

    assert!(list.contains_strong(&etag_a));
    assert!(!list.contains_strong(&etag_b)); // W/"b" は strong compare で false
}

#[test]
fn test_parse_etag_list_with_comma_in_tag() {
    // etagc はカンマを含み得る (0x2C は %x23-7E の範囲内)
    let list = parse_etag_list("\"a,b\", \"c\"").expect("ETag のパースは成功するはず (実装バグ)");
    match list {
        ETagList::Tags(tags) => {
            assert_eq!(tags.len(), 2);
            assert_eq!(tags[0].tag(), "a,b");
            assert_eq!(tags[1].tag(), "c");
        }
        _ => panic!("expected Tags"),
    }
}

#[test]
fn test_parse_etag_list_weak_with_comma_in_tag() {
    let list = parse_etag_list("W/\"x,y\", \"z\"").expect("ETag のパースは成功するはず (実装バグ)");
    match list {
        ETagList::Tags(tags) => {
            assert_eq!(tags.len(), 2);
            assert_eq!(tags[0].tag(), "x,y");
            assert!(tags[0].is_weak());
            assert_eq!(tags[1].tag(), "z");
        }
        _ => panic!("expected Tags"),
    }
}

#[test]
fn test_etag_list_display() {
    let list = parse_etag_list("\"a\", \"b\"").expect("ETag のパースは成功するはず (実装バグ)");
    assert_eq!(list.to_string(), "\"a\", \"b\"");

    let any = parse_etag_list("*").expect("ETag のパースは成功するはず (実装バグ)");
    assert_eq!(any.to_string(), "*");
}

// ========================================
// obs-text (U+0080 以上) の char 単位走査検証
// ========================================

#[test]
fn test_etag_obs_text_parse() {
    // obs-text (U+0080 以上) を含む ETag が正常にパースされる
    let etag = EntityTag::parse("\"v\u{00A9}\"").expect("ETag のパースは成功するはず (実装バグ)");
    assert!(etag.is_strong());
    assert_eq!(etag.tag(), "v\u{00A9}");
}

#[test]
fn test_etag_obs_text_roundtrip() {
    // obs-text を含む ETag の Display → 再パースのラウンドトリップ
    let etag = EntityTag::parse("\"v\u{00A9}\"").expect("ETag のパースは成功するはず (実装バグ)");
    let displayed = etag.to_string();
    let reparsed = EntityTag::parse(&displayed).expect("ETag のパースは成功するはず (実装バグ)");
    assert_eq!(etag.tag(), reparsed.tag());
    assert_eq!(etag.is_weak(), reparsed.is_weak());
}

#[test]
fn test_etag_multibyte_char_parse() {
    // マルチバイト文字 (U+3042 "あ") を含む ETag のパース
    let etag = EntityTag::parse("\"v\u{3042}\"").expect("ETag のパースは成功するはず (実装バグ)");
    assert_eq!(etag.tag(), "v\u{3042}");

    // strong/weak ビルダーでも受理される
    let strong = EntityTag::strong("v\u{3042}").expect("ETag のパースは成功するはず (実装バグ)");
    assert_eq!(strong.tag(), "v\u{3042}");
    let weak = EntityTag::weak("v\u{3042}").expect("ETag のパースは成功するはず (実装バグ)");
    assert_eq!(weak.tag(), "v\u{3042}");
    assert!(weak.is_weak());
}

#[test]
fn test_etag_cr_lf_nul_rejected() {
    // CR/LF/NUL を含む ETag タグは InvalidCharacter エラー
    assert!(EntityTag::strong("v\r").is_err());
    assert!(EntityTag::strong("v\n").is_err());
    assert!(EntityTag::strong("v\0").is_err());
    assert!(EntityTag::weak("v\r").is_err());
    assert!(EntityTag::weak("v\n").is_err());
    assert!(EntityTag::weak("v\0").is_err());
}
