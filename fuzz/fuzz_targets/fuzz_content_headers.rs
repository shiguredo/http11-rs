#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::content_encoding::ContentEncoding;
use shiguredo_http11::content_language::ContentLanguage;
use shiguredo_http11::content_location::ContentLocation;
use shiguredo_http11::digest_fields::{
    ContentDigest, ReprDigest, WantContentDigest, WantReprDigest,
};
use shiguredo_http11::expect::Expect;
use shiguredo_http11::host::Host;
use shiguredo_http11::trailer::Trailer;
use shiguredo_http11::upgrade::Upgrade;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        if let Ok(value) = ContentEncoding::parse(s) {
            for encoding in value.encodings() {
                let _ = encoding.as_str();
            }
            let _ = value.has_gzip();
            let _ = value.has_deflate();
            let _ = value.has_compress();
            let _ = value.has_identity();
            let displayed = value.to_string();
            let _ = ContentEncoding::parse(&displayed);
        }

        if let Ok(value) = ContentLanguage::parse(s) {
            let _ = value.tags();
            let displayed = value.to_string();
            let _ = ContentLanguage::parse(&displayed);
        }

        if let Ok(value) = ContentLocation::parse(s) {
            let _ = value.uri();
            let _ = value.uri().as_str();
            let displayed = value.to_string();
            let _ = ContentLocation::parse(&displayed);
        }

        if let Ok(value) = Trailer::parse(s) {
            let _ = value.fields();
            let displayed = value.to_string();
            let _ = Trailer::parse(&displayed);
        }

        if let Ok(value) = Host::parse(s) {
            let _ = value.host();
            let _ = value.port();
            let _ = value.is_ipv6();
            let displayed = value.to_string();
            let _ = Host::parse(&displayed);
        }

        if let Ok(value) = Expect::parse(s) {
            let _ = value.has_100_continue();
            for item in value.items() {
                let _ = item.token();
                let _ = item.value();
                let _ = item.is_100_continue();
            }
            let displayed = value.to_string();
            let _ = Expect::parse(&displayed);
        }

        if let Ok(value) = Upgrade::parse(s) {
            let _ = value.has_protocol("websocket");
            for protocol in value.protocols() {
                let _ = protocol.name();
                let _ = protocol.version();
            }
            let displayed = value.to_string();
            let _ = Upgrade::parse(&displayed);
        }

        if let Ok(value) = ContentDigest::parse(s) {
            for item in value.items() {
                let _ = item.algorithm();
                let _ = item.value();
                let _ = item.value().bytes();
                let _ = value.get(item.algorithm());
            }
            let displayed = value.to_string();
            let _ = ContentDigest::parse(&displayed);
        }

        if let Ok(value) = ReprDigest::parse(s) {
            for item in value.items() {
                let _ = item.algorithm();
                let _ = item.value();
                let _ = item.value().bytes();
                let _ = value.get(item.algorithm());
            }
            let displayed = value.to_string();
            let _ = ReprDigest::parse(&displayed);
        }

        if let Ok(value) = WantContentDigest::parse(s) {
            for item in value.items() {
                let _ = item.algorithm();
                let _ = item.weight();
                let _ = value.get(item.algorithm());
            }
            let displayed = value.to_string();
            let _ = WantContentDigest::parse(&displayed);
        }

        if let Ok(value) = WantReprDigest::parse(s) {
            for item in value.items() {
                let _ = item.algorithm();
                let _ = item.weight();
                let _ = value.get(item.algorithm());
            }
            let displayed = value.to_string();
            let _ = WantReprDigest::parse(&displayed);
        }
    }
});
