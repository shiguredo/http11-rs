#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::auth::{
    AuthChallenge, Authorization, BasicAuth, BearerChallenge, BearerToken, DigestAuth,
    DigestChallenge, ProxyAuthenticate, ProxyAuthorization, WwwAuthenticate,
};

fuzz_target!(|data: &[u8]| {
    // UTF-8 文字列として解釈できる場合のみテスト
    if let Ok(s) = std::str::from_utf8(data) {
        // BasicAuth パース
        if let Ok(auth) = BasicAuth::parse(s) {
            let _ = auth.username();
            let _ = auth.password();

            // Display ラウンドトリップ
            let displayed = auth.to_header_value();
            if let Ok(reparsed) = BasicAuth::parse(&displayed) {
                assert_eq!(auth.username(), reparsed.username());
                assert_eq!(auth.password(), reparsed.password());
            }
        }

        // WwwAuthenticate パース
        if let Ok(auth) = WwwAuthenticate::parse(s) {
            let _ = auth.realm();
            let _ = auth.charset();

            // Display ラウンドトリップ
            let displayed = auth.to_string();
            if let Ok(reparsed) = WwwAuthenticate::parse(&displayed) {
                assert_eq!(auth.realm(), reparsed.realm());
            }
        }

        // DigestAuth パース
        if let Ok(auth) = DigestAuth::parse(s) {
            let _ = auth.username();
            let _ = auth.realm();
            let _ = auth.nonce();
            let _ = auth.uri();
            let _ = auth.response();

            let displayed = auth.to_header_value();
            let _ = DigestAuth::parse(&displayed);
        }

        // DigestChallenge パース
        if let Ok(challenge) = DigestChallenge::parse(s) {
            let _ = challenge.realm();
            let _ = challenge.nonce();

            let displayed = challenge.to_header_value();
            let _ = DigestChallenge::parse(&displayed);
        }

        // BearerToken パース
        if let Ok(token) = BearerToken::parse(s) {
            let _ = token.token();
            let displayed = token.to_header_value();
            let _ = BearerToken::parse(&displayed);
        }

        // BearerChallenge パース
        if let Ok(challenge) = BearerChallenge::parse(s) {
            let _ = challenge.param("realm");
            let displayed = challenge.to_header_value();
            let _ = BearerChallenge::parse(&displayed);
        }

        // Authorization / AuthChallenge
        if let Ok(auth) = Authorization::parse(s) {
            let displayed = auth.to_header_value();
            let _ = Authorization::parse(&displayed);
        }
        if let Ok(challenge) = AuthChallenge::parse(s) {
            let displayed = challenge.to_header_value();
            let _ = AuthChallenge::parse(&displayed);
        }

        // Proxy headers
        if let Ok(auth) = ProxyAuthorization::parse(s) {
            let displayed = auth.to_header_value();
            let _ = ProxyAuthorization::parse(&displayed);
        }
        if let Ok(challenge) = ProxyAuthenticate::parse(s) {
            let displayed = challenge.to_header_value();
            let _ = ProxyAuthenticate::parse(&displayed);
        }
    }
});
