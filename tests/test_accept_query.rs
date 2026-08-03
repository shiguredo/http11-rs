//! Accept-Query ヘッダーのユニットテスト (RFC 10008 Section 3 / RFC 9651)

use shiguredo_http11::accept_query::{AcceptQuery, AcceptQueryError};

// ========================================
// AcceptQueryError のテスト
// ========================================

#[test]
fn test_accept_query_error_display() {
    let errors = [
        (
            AcceptQueryError::InvalidFormat,
            "invalid Accept-Query format",
        ),
        (AcceptQueryError::InvalidMediaRange, "invalid media range"),
        (AcceptQueryError::InvalidParameter, "invalid parameter"),
        (AcceptQueryError::UnterminatedQuote, "unterminated string"),
        (
            AcceptQueryError::UnsupportedItemType,
            "unsupported item type",
        ),
    ];

    for (error, expected) in errors {
        assert_eq!(error.to_string(), expected);
    }
}

// ========================================
// RFC 10008 の例
// ========================================

// Section 3 行 447: Token + String 混在、パラメータ付き
// Accept-Query: "application/jsonpath", application/sql;charset="UTF-8"
#[test]
fn test_rfc10008_section3_example() {
    let aq = AcceptQuery::parse("\"application/jsonpath\", application/sql;charset=\"UTF-8\"")
        .expect("Accept-Query のパースは成功するはず (実装バグ)");
    assert_eq!(aq.items().len(), 2);

    // 1 つ目: String 形式の application/jsonpath
    let item0 = &aq.items()[0];
    assert_eq!(item0.media_type(), "application");
    assert_eq!(item0.subtype(), "jsonpath");
    assert!(item0.parameters().is_empty());

    // 2 つ目: Token 形式の application/sql + charset パラメータ
    let item1 = &aq.items()[1];
    assert_eq!(item1.media_type(), "application");
    assert_eq!(item1.subtype(), "sql");
    assert_eq!(item1.parameters().len(), 1);
    assert_eq!(item1.parameters()[0].0, "charset");
    assert_eq!(item1.parameters()[0].1, "UTF-8");
}

// Appendix A.3 行 672: Token 形式の例
// Accept-Query: application/x-www-form-urlencoded, application/sql
#[test]
fn test_rfc10008_appendix_a3_token_form() {
    let aq = AcceptQuery::parse("application/x-www-form-urlencoded, application/sql")
        .expect("Accept-Query のパースは成功するはず (実装バグ)");
    assert_eq!(aq.items().len(), 2);

    assert_eq!(aq.items()[0].media_type(), "application");
    assert_eq!(aq.items()[0].subtype(), "x-www-form-urlencoded");

    assert_eq!(aq.items()[1].media_type(), "application");
    assert_eq!(aq.items()[1].subtype(), "sql");
}

// Appendix A.5 行 835: String 形式の例
// Accept-Query: "application/sql", "application/xslt+xml"
#[test]
fn test_rfc10008_appendix_a5_string_form() {
    let aq = AcceptQuery::parse("\"application/sql\", \"application/xslt+xml\"")
        .expect("Accept-Query のパースは成功するはず (実装バグ)");
    assert_eq!(aq.items().len(), 2);

    assert_eq!(aq.items()[0].media_type(), "application");
    assert_eq!(aq.items()[0].subtype(), "sql");

    assert_eq!(aq.items()[1].media_type(), "application");
    assert_eq!(aq.items()[1].subtype(), "xslt+xml");
}

// Appendix A.6 行 1036: String 形式の例
// Accept-Query: "application/jsonpath", "application/xslt+xml"
#[test]
fn test_rfc10008_appendix_a6_string_form() {
    let aq = AcceptQuery::parse("\"application/jsonpath\", \"application/xslt+xml\"")
        .expect("Accept-Query のパースは成功するはず (実装バグ)");
    assert_eq!(aq.items().len(), 2);

    assert_eq!(aq.items()[0].media_type(), "application");
    assert_eq!(aq.items()[0].subtype(), "jsonpath");

    assert_eq!(aq.items()[1].media_type(), "application");
    assert_eq!(aq.items()[1].subtype(), "xslt+xml");
}

// ========================================
// Display ラウンドトリップ
// ========================================

// Token 形式でパースした media range は Token 形式で Display される
#[test]
fn test_display_token_form() {
    let aq = AcceptQuery::parse("application/sql")
        .expect("Accept-Query のパースは成功するはず (実装バグ)");
    assert_eq!(aq.to_string(), "application/sql");
}

// String 形式でパースしても Token 表現可能なら Token に正規化される
#[test]
fn test_display_string_normalized_to_token() {
    let aq = AcceptQuery::parse("\"application/sql\"")
        .expect("Accept-Query のパースは成功するはず (実装バグ)");
    assert_eq!(aq.to_string(), "application/sql");
}

// 先頭数字の media range は String 形式で Display される
#[test]
fn test_display_leading_digit_string_form() {
    let aq =
        AcceptQuery::parse("\"3gpp/*\"").expect("Accept-Query のパースは成功するはず (実装バグ)");
    assert_eq!(aq.to_string(), "\"3gpp/*\"");
}

// パラメータ付きの Display (RFC 10008 Section 3 の例に準拠)
#[test]
fn test_display_with_parameters() {
    let aq = AcceptQuery::parse("application/sql;charset=\"UTF-8\"")
        .expect("Accept-Query のパースは成功するはず (実装バグ)");
    assert_eq!(aq.to_string(), "application/sql;charset=UTF-8");
}

// 空パラメータ値は SF String `""` で Display される
#[test]
fn test_display_empty_param_value() {
    let aq = AcceptQuery::parse("application/sql;charset=\"\"")
        .expect("Accept-Query のパースは成功するはず (実装バグ)");
    assert_eq!(aq.to_string(), "application/sql;charset=\"\"");
}

// Display ラウンドトリップ: parse(Display(x)) == x (意味等価)
#[test]
fn test_display_roundtrip_token() {
    let aq = AcceptQuery::parse("application/sql, text/html")
        .expect("Accept-Query のパースは成功するはず (実装バグ)");
    let displayed = aq.to_string();
    let reparsed =
        AcceptQuery::parse(&displayed).expect("Accept-Query のパースは成功するはず (実装バグ)");
    assert_eq!(aq, reparsed);
}

#[test]
fn test_display_roundtrip_mixed() {
    let aq = AcceptQuery::parse("\"application/jsonpath\", application/sql;charset=\"UTF-8\"")
        .expect("Accept-Query のパースは成功するはず (実装バグ)");
    let displayed = aq.to_string();
    let reparsed =
        AcceptQuery::parse(&displayed).expect("Accept-Query のパースは成功するはず (実装バグ)");
    assert_eq!(aq, reparsed);
}

#[test]
fn test_display_roundtrip_leading_digit() {
    let aq = AcceptQuery::parse("\"3gpp/*\", application/sql")
        .expect("Accept-Query のパースは成功するはず (実装バグ)");
    let displayed = aq.to_string();
    let reparsed =
        AcceptQuery::parse(&displayed).expect("Accept-Query のパースは成功するはず (実装バグ)");
    assert_eq!(aq, reparsed);
}

// ========================================
// 空入力
// ========================================

#[test]
fn test_parse_empty_returns_empty_list() {
    let aq = AcceptQuery::parse("").expect("Accept-Query のパースは成功するはず (実装バグ)");
    assert!(aq.items().is_empty());
}

#[test]
fn test_parse_only_spaces_returns_empty_list() {
    let aq = AcceptQuery::parse("   ").expect("Accept-Query のパースは成功するはず (実装バグ)");
    assert!(aq.items().is_empty());
}

// ========================================
// エラーケース
// ========================================

// trailing comma
#[test]
fn test_error_trailing_comma() {
    assert_eq!(
        AcceptQuery::parse("application/sql,"),
        Err(AcceptQueryError::InvalidFormat),
    );
}

// 連続カンマ
#[test]
fn test_error_consecutive_comma() {
    assert_eq!(
        AcceptQuery::parse("application/sql,,text/html"),
        Err(AcceptQueryError::InvalidFormat),
    );
}

// 先頭カンマ
#[test]
fn test_error_leading_comma() {
    assert_eq!(
        AcceptQuery::parse(",application/sql"),
        Err(AcceptQueryError::InvalidFormat),
    );
}

// 未消費残り文字 (カンマなしで次の要素が続く)
#[test]
fn test_error_unconsumed_remainder() {
    assert_eq!(
        AcceptQuery::parse("application/sql text/html"),
        Err(AcceptQueryError::InvalidFormat),
    );
}

// Inner List は不可
#[test]
fn test_error_inner_list() {
    assert_eq!(
        AcceptQuery::parse("(application/sql)"),
        Err(AcceptQueryError::UnsupportedItemType),
    );
}

// Integer は不可
#[test]
fn test_error_integer() {
    assert_eq!(
        AcceptQuery::parse("42"),
        Err(AcceptQueryError::UnsupportedItemType),
    );
}

// Decimal は不可
#[test]
fn test_error_decimal() {
    assert_eq!(
        AcceptQuery::parse("4.2"),
        Err(AcceptQueryError::UnsupportedItemType),
    );
}

// Boolean は不可
#[test]
fn test_error_boolean() {
    assert_eq!(
        AcceptQuery::parse("?1"),
        Err(AcceptQueryError::UnsupportedItemType),
    );
    assert_eq!(
        AcceptQuery::parse("?0"),
        Err(AcceptQueryError::UnsupportedItemType),
    );
}

// Byte Sequence は不可
#[test]
fn test_error_byte_sequence() {
    assert_eq!(
        AcceptQuery::parse(":YWJj:"),
        Err(AcceptQueryError::UnsupportedItemType),
    );
}

// Date は不可
#[test]
fn test_error_date() {
    assert_eq!(
        AcceptQuery::parse("@1234567890"),
        Err(AcceptQueryError::UnsupportedItemType),
    );
}

// Display String は不可
#[test]
fn test_error_display_string() {
    assert_eq!(
        AcceptQuery::parse("%\"abc\""),
        Err(AcceptQueryError::UnsupportedItemType),
    );
}

// `*/subtype` は不可
#[test]
fn test_error_star_slash_subtype() {
    assert_eq!(
        AcceptQuery::parse("*/html"),
        Err(AcceptQueryError::InvalidMediaRange),
    );
}

// type/subtype が HTTP token として不正な文字を含む (`:`)
#[test]
fn test_error_media_range_with_colon() {
    assert_eq!(
        AcceptQuery::parse("text/html:extra"),
        Err(AcceptQueryError::InvalidMediaRange),
    );
}

// type/subtype が HTTP token として不正な文字を含む (追加の `/`)
#[test]
fn test_error_media_range_with_extra_slash() {
    assert_eq!(
        AcceptQuery::parse("text/html/foo"),
        Err(AcceptQueryError::InvalidMediaRange),
    );
}

// SF String 内 HTAB は不許可
#[test]
fn test_error_sf_string_htab() {
    assert_eq!(
        AcceptQuery::parse("\"application/sql\t\""),
        Err(AcceptQueryError::InvalidFormat),
    );
}

// SF String 内 obs-text は不許可 (RFC 9651 は ASCII のみ)
#[test]
fn test_error_sf_string_obs_text() {
    assert_eq!(
        AcceptQuery::parse("\"application/sql\u{0080}\""),
        Err(AcceptQueryError::InvalidFormat),
    );
}

// SF String 不正エスケープ (`\a`)
#[test]
fn test_error_sf_string_invalid_escape() {
    assert_eq!(
        AcceptQuery::parse("\"application/sql\\a\""),
        Err(AcceptQueryError::InvalidFormat),
    );
}

// SF Parameter key 大文字は不可
#[test]
fn test_error_param_key_uppercase() {
    assert_eq!(
        AcceptQuery::parse("application/sql;Charset=UTF-8"),
        Err(AcceptQueryError::InvalidParameter),
    );
}

// 値なしパラメータ (`;key`) は Boolean true 扱いで不可
#[test]
fn test_error_param_valueless() {
    assert_eq!(
        AcceptQuery::parse("application/sql;key"),
        Err(AcceptQueryError::InvalidParameter),
    );
}

// 明示的 Boolean parameter value (`;key=?1`) は不可
#[test]
fn test_error_param_boolean_value() {
    assert_eq!(
        AcceptQuery::parse("application/sql;key=?1"),
        Err(AcceptQueryError::InvalidParameter),
    );
}

// parameter value が Integer の場合は不可
#[test]
fn test_error_param_integer_value() {
    assert_eq!(
        AcceptQuery::parse("application/sql;key=42"),
        Err(AcceptQueryError::InvalidParameter),
    );
}

// parameter value が Byte Sequence の場合は不可
#[test]
fn test_error_param_byte_sequence_value() {
    assert_eq!(
        AcceptQuery::parse("application/sql;key=:abc:"),
        Err(AcceptQueryError::InvalidParameter),
    );
}

// parameter value が Date の場合は不可
#[test]
fn test_error_param_date_value() {
    assert_eq!(
        AcceptQuery::parse("application/sql;key=@123"),
        Err(AcceptQueryError::InvalidParameter),
    );
}

// parameter value が Display String の場合は不可
#[test]
fn test_error_param_display_string_value() {
    assert_eq!(
        AcceptQuery::parse("application/sql;key=%\"abc\""),
        Err(AcceptQueryError::InvalidParameter),
    );
}

// parameter value の `=` 直後 EOF
#[test]
fn test_error_param_equals_eof() {
    assert_eq!(
        AcceptQuery::parse("application/sql;key="),
        Err(AcceptQueryError::InvalidParameter),
    );
}

// parameter value の unterminated SF String
#[test]
fn test_error_param_unterminated_string() {
    assert_eq!(
        AcceptQuery::parse("application/sql;charset=\"abc"),
        Err(AcceptQueryError::UnterminatedQuote),
    );
}

// `;` 直後 EOF (parameter key なし)
#[test]
fn test_error_semicolon_eof() {
    assert_eq!(
        AcceptQuery::parse("application/sql;"),
        Err(AcceptQueryError::InvalidParameter),
    );
}

// media range に `/` が含まれない
#[test]
fn test_error_media_range_no_slash() {
    assert_eq!(
        AcceptQuery::parse("text"),
        Err(AcceptQueryError::InvalidMediaRange),
    );
}

// SF String の backslash 直後 EOF
#[test]
fn test_error_sf_string_backslash_eof() {
    assert_eq!(
        AcceptQuery::parse("\"application/sql\\"),
        Err(AcceptQueryError::InvalidFormat),
    );
}

// SF String 内 DEL (0x7F) は不許可 (RFC 9651 Section 4.2.5 step 4.4)
#[test]
fn test_error_sf_string_del() {
    assert_eq!(
        AcceptQuery::parse("\"application/sql\u{007F}\""),
        Err(AcceptQueryError::InvalidFormat),
    );
}

// 重複 parameter key は RFC 9651 Section 4.2.3.2 step 7 に従い
// 最後の値で上書き (last-wins) される
#[test]
fn test_duplicate_param_key_last_wins() {
    let aq = AcceptQuery::parse("application/sql;key=v1;key=v2")
        .expect("Accept-Query のパースは成功するはず (実装バグ)");
    assert_eq!(aq.items()[0].parameters().len(), 1);
    assert_eq!(aq.items()[0].parameters()[0].0, "key");
    assert_eq!(aq.items()[0].parameters()[0].1, "v2");
}

// 閉じ DQUOTE がない
#[test]
fn test_error_unterminated_quote() {
    assert_eq!(
        AcceptQuery::parse("\"application/sql"),
        Err(AcceptQueryError::UnterminatedQuote),
    );
}

// 非 ASCII バイト (UTF-8 マルチバイト) は fail
#[test]
fn test_error_non_ascii() {
    assert_eq!(
        AcceptQuery::parse("application/sql\u{3042}"),
        Err(AcceptQueryError::InvalidFormat),
    );
}

// ========================================
// ワイルドカード
// ========================================

#[test]
fn test_wildcard_star_star() {
    let aq = AcceptQuery::parse("*/*").expect("Accept-Query のパースは成功するはず (実装バグ)");
    assert_eq!(aq.items()[0].media_type(), "*");
    assert_eq!(aq.items()[0].subtype(), "*");
}

#[test]
fn test_wildcard_type_star() {
    let aq = AcceptQuery::parse("text/*").expect("Accept-Query のパースは成功するはず (実装バグ)");
    assert_eq!(aq.items()[0].media_type(), "text");
    assert_eq!(aq.items()[0].subtype(), "*");
}

// ========================================
// 小文字正規化
// ========================================

// media type / subtype は小文字に正規化される
#[test]
fn test_lowercase_normalization() {
    let aq =
        AcceptQuery::parse("TEXT/HTML").expect("Accept-Query のパースは成功するはず (実装バグ)");
    assert_eq!(aq.items()[0].media_type(), "text");
    assert_eq!(aq.items()[0].subtype(), "html");
}

// パラメータ key は小文字のみ許可 (大文字はエラー)
// パラメータ value はそのまま保持 (Token の場合)
#[test]
fn test_param_value_preserved() {
    let aq = AcceptQuery::parse("application/sql;charset=UTF-8")
        .expect("Accept-Query のパースは成功するはず (実装バグ)");
    assert_eq!(aq.items()[0].parameters()[0].0, "charset");
    assert_eq!(aq.items()[0].parameters()[0].1, "UTF-8");
}

// ========================================
// 複数要素
// ========================================

#[test]
fn test_multiple_items() {
    let aq = AcceptQuery::parse("application/sql, text/html, application/json")
        .expect("Accept-Query のパースは成功するはず (実装バグ)");
    assert_eq!(aq.items().len(), 3);
}

// OWS (SP / HTAB) がメンバー間で許容される
#[test]
fn test_ows_between_members() {
    let aq = AcceptQuery::parse("application/sql,\t text/html")
        .expect("Accept-Query のパースは成功するはず (実装バグ)");
    assert_eq!(aq.items().len(), 2);
}

// 先頭の SP は discard される (RFC 9651 Section 4.2 step 2 は SP のみ)
#[test]
fn test_leading_sp_discarded() {
    let aq = AcceptQuery::parse("   application/sql")
        .expect("Accept-Query のパースは成功するはず (実装バグ)");
    assert_eq!(aq.items().len(), 1);
    assert_eq!(aq.items()[0].subtype(), "sql");
}

// 先頭の HTAB は RFC 9651 Section 4.2 step 2 (SP のみ) で許容されない
#[test]
fn test_error_leading_htab() {
    assert_eq!(
        AcceptQuery::parse("\tapplication/sql"),
        Err(AcceptQueryError::InvalidFormat),
    );
}
