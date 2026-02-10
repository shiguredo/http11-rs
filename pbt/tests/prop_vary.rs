//! Vary ヘッダーのプロパティテスト (vary.rs)

use proptest::prelude::*;
use shiguredo_http11::vary::Vary;

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

// Vary のラウンドトリップ
proptest! {
    #[test]
    fn prop_vary_roundtrip(value in prop_oneof![Just("*".to_string()), proptest::collection::vec(token_string(8), 1..5).prop_map(|tokens| tokens.join(", "))]) {
        let parsed = Vary::parse(&value).unwrap();
        let displayed = parsed.to_string();
        let reparsed = Vary::parse(&displayed).unwrap();
        prop_assert_eq!(parsed, reparsed);
    }
}
