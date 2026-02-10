//! Upgrade ヘッダーのプロパティテスト (upgrade.rs)

use proptest::prelude::*;
use shiguredo_http11::upgrade::Upgrade;

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

// Upgrade のラウンドトリップ
proptest! {
    #[test]
    fn prop_upgrade_roundtrip(items in proptest::collection::vec((token_string(8), prop::option::of(token_string(8))), 1..4)) {
        let first_protocol = items[0].0.clone();
        let mut parts = Vec::new();

        for (protocol, version) in items {
            let part = match version {
                Some(version) => format!("{}/{}", protocol, version),
                None => protocol,
            };
            parts.push(part);
        }

        let header = parts.join(", ");
        let parsed = Upgrade::parse(&header).unwrap();
        let displayed = parsed.to_string();
        let reparsed = Upgrade::parse(&displayed).unwrap();
        prop_assert_eq!(&parsed, &reparsed);
        prop_assert!(parsed.has_protocol(&first_protocol));
    }
}
