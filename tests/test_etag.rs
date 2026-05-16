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
    let list = parse_etag_list("*").unwrap();
    assert!(list.is_any());
}

#[test]
fn test_etag_list_contains() {
    let list = parse_etag_list("\"a\", W/\"b\"").unwrap();
    let etag_a = EntityTag::strong("a").unwrap();
    let etag_b = EntityTag::strong("b").unwrap();
    let etag_c = EntityTag::strong("c").unwrap();

    assert!(list.contains_weak(&etag_a));
    assert!(list.contains_weak(&etag_b));
    assert!(!list.contains_weak(&etag_c));

    assert!(list.contains_strong(&etag_a));
    assert!(!list.contains_strong(&etag_b)); // W/"b" は strong compare で false
}

#[test]
fn test_parse_etag_list_with_comma_in_tag() {
    // etagc はカンマを含み得る (0x2C は %x23-7E の範囲内)
    let list = parse_etag_list("\"a,b\", \"c\"").unwrap();
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
    let list = parse_etag_list("W/\"x,y\", \"z\"").unwrap();
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
    let list = parse_etag_list("\"a\", \"b\"").unwrap();
    assert_eq!(list.to_string(), "\"a\", \"b\"");

    let any = parse_etag_list("*").unwrap();
    assert_eq!(any.to_string(), "*");
}
