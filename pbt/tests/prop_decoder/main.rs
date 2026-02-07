//! Decoder のプロパティテスト (decoder/)

mod body;
mod head;
mod request;
mod response;

use proptest::prelude::*;

pub(crate) fn http_method() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("GET".to_string()),
        Just("POST".to_string()),
        Just("PUT".to_string()),
        Just("DELETE".to_string()),
        Just("HEAD".to_string()),
        Just("OPTIONS".to_string()),
        Just("PATCH".to_string()),
    ]
}

pub(crate) fn http_uri() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("/".to_string()),
        "/[a-zA-Z0-9/_.-]{1,64}".prop_map(|s| s),
    ]
}

pub(crate) fn status_code() -> impl Strategy<Value = u16> {
    prop_oneof![
        100u16..=101,
        200u16..=206,
        300u16..=308,
        400u16..=451,
        500u16..=511,
    ]
}

pub(crate) fn reason_phrase() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("OK".to_string()),
        Just("Not Found".to_string()),
        Just("Internal Server Error".to_string()),
        "[A-Za-z ]{1,32}".prop_map(|s| s),
    ]
}

pub(crate) fn body() -> impl Strategy<Value = Vec<u8>> {
    proptest::collection::vec(any::<u8>(), 0..256)
}

/// 無効なヘッダー名の文字を生成する Strategy
/// 注: `:` はヘッダーの区切り文字として解釈されるため除外
pub(crate) fn invalid_header_name_char() -> impl Strategy<Value = char> {
    prop_oneof![
        Just('@'),
        Just('['),
        Just(']'),
        Just('\\'),
        Just('{'),
        Just('}'),
        Just('<'),
        Just('>'),
        Just('('),
        Just(')'),
        Just(','),
        Just(';'),
        Just('"'),
        Just('/'),
        Just('?'),
        Just('='),
    ]
}

/// 有効なヘッダー名の文字を生成する Strategy
pub(crate) fn valid_header_name_special_char() -> impl Strategy<Value = char> {
    prop_oneof![
        Just('!'),
        Just('#'),
        Just('$'),
        Just('%'),
        Just('&'),
        Just('\''),
        Just('*'),
        Just('+'),
        Just('^'),
        Just('`'),
        Just('|'),
        Just('~'),
        Just('-'),
        Just('_'),
        Just('.'),
    ]
}

/// Transfer-Encoding トークン生成 Strategy
pub(crate) fn transfer_encoding_token() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("chunked".to_string()),
        Just("gzip".to_string()),
        Just("deflate".to_string()),
        Just("compress".to_string()),
        Just("identity".to_string()),
    ]
}
