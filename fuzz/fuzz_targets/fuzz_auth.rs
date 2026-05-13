//! 認証関連ヘッダーのパニック安全性と Display ラウンドトリップを検証する
//!
//! - BasicAuth, DigestAuth, BearerToken の各認証スキームを任意入力でパースする
//! - WwwAuthenticate, DigestChallenge, BearerChallenge のチャレンジをパースする
//! - Authorization, ProxyAuthorization, ProxyAuthenticate の汎用パーサーを検証する
//! - パース成功時はアクセサを呼び出し、Display 出力の再パースでラウンドトリップを確認する

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

            // Display ラウンドトリップ
            let displayed = auth.to_header_value();
            if let Ok(reparsed) = DigestAuth::parse(&displayed) {
                assert_eq!(auth.username(), reparsed.username());
                assert_eq!(auth.username_decoded(), reparsed.username_decoded());
                assert_eq!(auth.realm(), reparsed.realm());
                assert_eq!(auth.nonce(), reparsed.nonce());
                assert_eq!(auth.uri(), reparsed.uri());
                assert_eq!(auth.response(), reparsed.response());
                for name in [
                    "opaque",
                    "cnonce",
                    "nc",
                    "qop",
                    "algorithm",
                ] {
                    assert_eq!(auth.param(name), reparsed.param(name));
                }
            }
        }

        // DigestChallenge パース
        if let Ok(challenge) = DigestChallenge::parse(s) {
            let _ = challenge.realm();
            let _ = challenge.nonce();

            // Display ラウンドトリップ
            let displayed = challenge.to_header_value();
            if let Ok(reparsed) = DigestChallenge::parse(&displayed) {
                assert_eq!(challenge.realm(), reparsed.realm());
                assert_eq!(challenge.nonce(), reparsed.nonce());
                for name in [
                    "opaque",
                    "domain",
                    "qop",
                    "algorithm",
                    "userhash",
                    "stale",
                ] {
                    assert_eq!(challenge.param(name), reparsed.param(name));
                }
            }
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
