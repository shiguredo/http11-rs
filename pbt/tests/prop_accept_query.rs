//! Accept-Query ヘッダーのプロパティテスト (RFC 10008 Section 3 / RFC 9651)

use proptest::prelude::*;
use shiguredo_http11::accept_query::AcceptQuery;

// ========================================
// Strategy 定義
// ========================================

// SF Token の先頭文字 (RFC 9651 Section 4.2.6): ALPHA または `*`
fn sf_token_first_char() -> impl Strategy<Value = char> {
    prop_oneof![
        prop::char::range('a', 'z'),
        prop::char::range('A', 'Z'),
        Just('*'),
    ]
}

// SF Token の後続文字 (RFC 9651 Section 4.2.6): tchar / `:` / `/`
fn sf_token_trailing_char() -> impl Strategy<Value = char> {
    prop_oneof![
        // tchar (RFC 9110 Section 5.6.2)
        prop::char::range('a', 'z'),
        prop::char::range('A', 'Z'),
        prop::char::range('0', '9'),
        Just('!'),
        Just('#'),
        Just('$'),
        Just('%'),
        Just('&'),
        Just('\''),
        Just('*'),
        Just('+'),
        Just('-'),
        Just('.'),
        Just('^'),
        Just('_'),
        Just('`'),
        Just('|'),
        Just('~'),
        // SF Token 拡張
        Just(':'),
        Just('/'),
    ]
}

// SF Token として妥当な文字列 (先頭 ALPHA/`*`、後続 tchar/`:`/`/`)
fn sf_token_string(max_len: usize) -> impl Strategy<Value = String> {
    (
        sf_token_first_char(),
        proptest::collection::vec(sf_token_trailing_char(), 0..max_len),
    )
        .prop_map(|(first, rest)| {
            let mut s = String::new();
            s.push(first);
            s.extend(rest);
            s
        })
}

// HTTP token (tchar only) の文字列。media range の type/subtype に使用する
// type は SF Token の先頭文字制約 (ALPHA/`*`) に従う必要があるため、
// 先頭を ALPHA に制限した type 用と、任意の tchar を許容する subtype 用を分ける
// いずれも tchar のみ (`:` `/` は SF Token 拡張だが media range では不可)
fn media_type_token(max_len: usize) -> impl Strategy<Value = String> {
    // 先頭は ALPHA (SF Token 先頭制約。`*` は `*/*` で別途扱う)
    let first = prop_oneof![prop::char::range('a', 'z'), prop::char::range('A', 'Z')];
    (first, proptest::collection::vec(tchar_char(), 0..max_len)).prop_map(|(f, rest)| {
        let mut s = String::new();
        s.push(f);
        s.extend(rest);
        s
    })
}

fn media_subtype_token(max_len: usize) -> impl Strategy<Value = String> {
    proptest::collection::vec(tchar_char(), 1..=max_len)
        .prop_map(|chars| chars.into_iter().collect())
}

// tchar (RFC 9110 Section 5.6.2)。SF Token 拡張 (`:` `/`) は含まない
fn tchar_char() -> impl Strategy<Value = char> {
    prop_oneof![
        prop::char::range('a', 'z'),
        prop::char::range('A', 'Z'),
        prop::char::range('0', '9'),
        Just('!'),
        Just('#'),
        Just('$'),
        Just('%'),
        Just('&'),
        Just('\''),
        Just('*'),
        Just('+'),
        Just('-'),
        Just('.'),
        Just('^'),
        Just('_'),
        Just('`'),
        Just('|'),
        Just('~'),
    ]
}

// media range (type/subtype)。Token 形式で表現可能なもの
// type の先頭は ALPHA である必要がある (SF Token 先頭制約)
fn media_range_token() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("*/*".to_string()),
        (media_type_token(8), media_subtype_token(8)).prop_map(|(t, s)| format!("{}/{}", t, s)),
        media_type_token(8).prop_map(|t| format!("{}/*", t)),
    ]
}

// 先頭数字の type で Token 表現不可能な media range (String 形式が必要)
fn media_range_string_only() -> impl Strategy<Value = String> {
    // 先頭数字の type (後続は tchar)
    let first = prop::char::range('0', '9');
    (
        first,
        proptest::collection::vec(tchar_char(), 0..7),
        media_subtype_token(8),
    )
        .prop_map(|(f, rest, s)| {
            let mut t = String::new();
            t.push(f);
            t.extend(rest);
            format!("{}/{}", t, s)
        })
}

// SF Parameter key (RFC 9651 Section 4.2.3.3):
// ( lcalpha / "*" ) *( lcalpha / DIGIT / "_" / "-" / "." / "*" )
fn sf_param_key() -> impl Strategy<Value = String> {
    let first = prop_oneof![prop::char::range('a', 'z'), Just('*')];
    let trailing = prop_oneof![
        prop::char::range('a', 'z'),
        prop::char::range('0', '9'),
        Just('_'),
        Just('-'),
        Just('.'),
        Just('*'),
    ];
    (first, proptest::collection::vec(trailing, 0..8)).prop_map(|(f, rest)| {
        let mut s = String::new();
        s.push(f);
        s.extend(rest);
        s
    })
}

// SF String の内容 (%x20-21 / %x23-5B / %x5D-7E、空文字列含む)
fn sf_string_value(max_len: usize) -> impl Strategy<Value = String> {
    proptest::collection::vec(
        prop_oneof![
            prop::char::range('\u{20}', '\u{21}'),
            prop::char::range('\u{23}', '\u{5B}'),
            prop::char::range('\u{5D}', '\u{7E}'),
        ],
        0..max_len,
    )
    .prop_map(|chars| chars.into_iter().collect())
}

// パラメータ値: SF Token または SF String
fn sf_param_value() -> impl Strategy<Value = String> {
    prop_oneof![sf_token_string(8), sf_string_value(8),]
}

// パラメータリスト (重複 key 許可: RFC 9651 Section 4.2.3.2 は last-wins 上書き)
fn parameters() -> impl Strategy<Value = Vec<(String, String)>> {
    proptest::collection::vec((sf_param_key(), sf_param_value()), 0..3)
}

// メディアレンジアイテム (Token 形式)
fn media_range_item_token() -> impl Strategy<Value = String> {
    (media_range_token(), parameters()).prop_map(|(range, params)| {
        if params.is_empty() {
            range
        } else {
            let mut s = range;
            for (k, v) in params {
                if can_be_sf_token(&v) {
                    s.push_str(&format!(";{}={}", k, v));
                } else {
                    s.push_str(&format!(";{}=\"{}\"", k, escape_sf_string(&v)));
                }
            }
            s
        }
    })
}

// メディアレンジアイテム (String 形式、先頭数字)
fn media_range_item_string() -> impl Strategy<Value = String> {
    (media_range_string_only(), parameters()).prop_map(|(range, params)| {
        let escaped = escape_sf_string(&range);
        let mut s = format!("\"{}\"", escaped);
        for (k, v) in params {
            if can_be_sf_token(&v) {
                s.push_str(&format!(";{}={}", k, v));
            } else {
                s.push_str(&format!(";{}=\"{}\"", k, escape_sf_string(&v)));
            }
        }
        s
    })
}

// 文字列が SF Token として表現可能か (Display 判定用)
fn can_be_sf_token(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let bytes = s.as_bytes();
    let first = bytes[0];
    if !(first.is_ascii_alphabetic() || first == b'*') {
        return false;
    }
    bytes[1..].iter().all(|&b| {
        matches!(
            b,
            b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*' | b'+' | b'-' | b'.' |
            b'0'..=b'9' | b'A'..=b'Z' | b'^' | b'_' | b'`' | b'a'..=b'z' | b'|' | b'~' |
            b':' | b'/'
        )
    })
}

// SF String 用のエスケープ (`"` と `\` をエスケープ)
fn escape_sf_string(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        if c == '"' || c == '\\' {
            result.push('\\');
        }
        result.push(c);
    }
    result
}

// ========================================
// Display ラウンドトリップテスト
// ========================================

// Token 形式の media range のラウンドトリップ (意味等価)
proptest! {
    #[test]
    fn prop_accept_query_roundtrip_token(
        items in proptest::collection::vec(media_range_item_token(), 1..4)
    ) {
        let header = items.join(", ");
        let parsed = match AcceptQuery::parse(&header) {
            Ok(v) => v,
            Err(e) => {
                prop_assert!(false, "パース失敗: {:?} 入力: {:?}", e, header);
                return Ok(());
            }
        };
        let displayed = parsed.to_string();
        let reparsed = AcceptQuery::parse(&displayed).unwrap();
        prop_assert_eq!(parsed, reparsed);
    }
}

// String 形式 (先頭数字) の media range のラウンドトリップ
proptest! {
    #[test]
    fn prop_accept_query_roundtrip_string(
        items in proptest::collection::vec(media_range_item_string(), 1..4)
    ) {
        let header = items.join(", ");
        let parsed = match AcceptQuery::parse(&header) {
            Ok(v) => v,
            Err(e) => {
                prop_assert!(false, "パース失敗: {:?} 入力: {:?}", e, header);
                return Ok(());
            }
        };
        let displayed = parsed.to_string();
        let reparsed = AcceptQuery::parse(&displayed).unwrap();
        prop_assert_eq!(parsed, reparsed);
    }
}

// Token 表現可能な media range をあえて String で渡す
proptest! {
    #[test]
    fn prop_accept_query_string_normalized_to_token(
        range in media_range_token()
    ) {
        let header = format!("\"{}\"", escape_sf_string(&range));
        let parsed = AcceptQuery::parse(&header).unwrap();
        // Display は Token 形式に正規化される
        let displayed = parsed.to_string();
        prop_assert!(!displayed.starts_with('"'), "Display は Token 形式になるべき: {}", displayed);
        // ラウンドトリップ
        let reparsed = AcceptQuery::parse(&displayed).unwrap();
        prop_assert_eq!(parsed, reparsed);
    }
}

// 先頭数字の type で Token 表現不可能な media range を String で渡す
proptest! {
    #[test]
    fn prop_accept_query_leading_digit_string_form(
        range in media_range_string_only()
    ) {
        let header = format!("\"{}\"", escape_sf_string(&range));
        let parsed = AcceptQuery::parse(&header).unwrap();
        // Display は String 形式で出力される
        let displayed = parsed.to_string();
        prop_assert!(displayed.starts_with('"'), "Display は String 形式になるべき: {}", displayed);
        // ラウンドトリップ
        let reparsed = AcceptQuery::parse(&displayed).unwrap();
        prop_assert_eq!(parsed, reparsed);
    }
}

// ========================================
// アクセサのテスト
// ========================================

// media_type / subtype / parameters アクセサ
proptest! {
    #[test]
    fn prop_accept_query_accessors(
        media_type in media_type_token(8),
        subtype in media_subtype_token(8),
    ) {
        let header = format!("{}/{}", media_type, subtype);
        let aq = AcceptQuery::parse(&header).unwrap();
        let item = &aq.items()[0];

        prop_assert_eq!(item.media_type(), media_type.to_ascii_lowercase());
        prop_assert_eq!(item.subtype(), subtype.to_ascii_lowercase());
        prop_assert!(item.parameters().is_empty());
    }
}

// パラメータ付きラウンドトリップ
proptest! {
    #[test]
    fn prop_accept_query_with_parameters_roundtrip(
        range in media_range_token(),
        params in parameters()
    ) {
        let mut header = range;
        for (k, v) in &params {
            if can_be_sf_token(v) {
                header.push_str(&format!(";{}={}", k, v));
            } else {
                header.push_str(&format!(";{}=\"{}\"", k, escape_sf_string(v)));
            }
        }
        let parsed = AcceptQuery::parse(&header).unwrap();
        let displayed = parsed.to_string();
        let reparsed = AcceptQuery::parse(&displayed).unwrap();
        prop_assert_eq!(parsed, reparsed);
    }
}

// 空入力は空リスト
proptest! {
    #[test]
    fn prop_accept_query_empty_is_empty_list(
        spaces in proptest::collection::vec(Just(' '), 0..8)
    ) {
        let input: String = spaces.into_iter().collect();
        let aq = AcceptQuery::parse(&input).unwrap();
        prop_assert!(aq.items().is_empty());
    }
}
