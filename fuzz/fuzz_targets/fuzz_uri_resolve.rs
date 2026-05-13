//! URI の resolve() と normalize() の任意 base/reference 組み合わせを検証する
//!
//! 既存の fuzz_uri は base が is_absolute なら reference を `/test` 固定で
//! resolve していたが、本 fuzz target は base と reference の両方を任意入力にする。
//!
//! 検証対象:
//! - `resolve(base, reference)` が任意 URI ペアでパニックしないこと
//! - `normalize(uri)` が任意 URI でパニックしないこと
//! - `resolve(resolve_result, reference)` の二段適用でもパニックしないこと
//!   (相対 URI 解決の連鎖)
//! - 同じ URI を 2 回 `normalize` した結果が冪等であること

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use shiguredo_http11::uri::{Uri, normalize, resolve};

#[derive(Arbitrary, Debug)]
struct FuzzUriResolve<'a> {
    base: &'a str,
    reference: &'a str,
}

fuzz_target!(|input: FuzzUriResolve| {
    let FuzzUriResolve { base, reference } = input;

    let Ok(base_uri) = Uri::parse(base) else {
        return;
    };
    let Ok(reference_uri) = Uri::parse(reference) else {
        return;
    };

    // resolve() のパニック安全性
    let resolved = match resolve(&base_uri, &reference_uri) {
        Ok(uri) => uri,
        Err(_) => return,
    };

    // 二段 resolve: 既に解決済みの URI を base にして再度 resolve
    let _ = resolve(&resolved, &reference_uri);

    // normalize() のパニック安全性と冪等性 (RFC 3986 Section 6.2.2)
    if let Ok(normalized) = normalize(&resolved)
        && let Ok(renormalized) = normalize(&normalized)
    {
        assert_eq!(
            normalized.as_str(),
            renormalized.as_str(),
            "normalize should be idempotent"
        );
    }

    // base 自身を normalize しても安全
    let _ = normalize(&base_uri);
});
