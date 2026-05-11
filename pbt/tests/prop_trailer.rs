//! Trailer ヘッダーのプロパティテスト (trailer.rs)

use proptest::prelude::*;
use shiguredo_http11::trailer::{Trailer, is_prohibited_trailer_field};

// HTTP トークン文字
fn token_char() -> impl Strategy<Value = char> {
    prop_oneof![
        prop::char::range('a', 'z'),
        prop::char::range('A', 'Z'),
        prop::char::range('0', '9'),
        Just('-'),
        Just('_'),
        Just('.'),
    ]
}

fn token_string(max_len: usize) -> impl Strategy<Value = String> {
    proptest::collection::vec(token_char(), 1..=max_len)
        .prop_map(|chars| chars.into_iter().collect())
}

/// trailer フィールド名として valid なトークン strategy
///
/// `is_prohibited_trailer_field` で reject される名前 (RFC 9110 Section 6.5.1 の
/// framing / routing / 認証 / リクエスト修飾子 / レスポンス制御 / 接続管理 /
/// コンテンツ形式) はラウンドトリップに使えないため除外する。乱数で `te` や
/// `expires` のような短い禁止名を踏むケースが Windows 環境などで顕在化していた。
fn allowed_trailer_token(max_len: usize) -> impl Strategy<Value = String> {
    token_string(max_len).prop_filter("禁止 trailer フィールド名は除外", |s| {
        !is_prohibited_trailer_field(s)
    })
}

// Trailer のラウンドトリップ
proptest! {
    #[test]
    fn prop_trailer_roundtrip(
        tokens in proptest::collection::vec(allowed_trailer_token(8), 1..5)
    ) {
        let header = tokens.join(", ");
        let parsed = Trailer::parse(&header).unwrap();
        let displayed = parsed.to_string();
        let reparsed = Trailer::parse(&displayed).unwrap();
        prop_assert_eq!(parsed, reparsed);
    }
}
