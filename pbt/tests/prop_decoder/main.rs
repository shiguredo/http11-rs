//! Decoder のプロパティテスト (decoder/)

mod body;
mod head;
mod request;
mod request_target;
mod response;

use shiguredo_http11::Method;

/// PBT 用の HTTP メソッド生成
///
/// 固定メソッド一覧から一様に選ぶ。
pub(crate) fn http_method(ctx: &mut noprop::TestCaseContext) -> Method {
    match noprop::sample_usize_in(ctx, 0..8) {
        0 => Method::GET,
        1 => Method::POST,
        2 => Method::PUT,
        3 => Method::DELETE,
        4 => Method::HEAD,
        5 => Method::OPTIONS,
        6 => Method::PATCH,
        _ => Method::QUERY,
    }
}

/// PBT 用の HTTP リクエストターゲット生成
///
/// "/" または `/[a-zA-Z0-9/_.-]{1,64}` 形式のパスを生成する。
pub(crate) fn http_uri(ctx: &mut noprop::TestCaseContext) -> String {
    match noprop::sample_usize_in(ctx, 0..2) {
        0 => "/".to_string(),
        _ => {
            let len = noprop::sample_usize_in(ctx, 1..=64);
            let mut s = String::with_capacity(len + 1);
            s.push('/');
            for _ in 0..len {
                s.push(uri_path_char(ctx));
            }
            s
        }
    }
}

/// URI パスの 1 文字を生成する
fn uri_path_char(ctx: &mut noprop::TestCaseContext) -> char {
    // 文字クラス [a-zA-Z0-9/_.-] のいずれかを一様に選ぶ
    match noprop::sample_usize_in(ctx, 0..4) {
        0 => char::from(b'A' + noprop::sample_usize_in(ctx, 0..26) as u8),
        1 => char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8),
        2 => char::from(b'0' + noprop::sample_usize_in(ctx, 0..10) as u8),
        _ => {
            const PATH_SPECIAL: &[u8] = b"/_.-";
            PATH_SPECIAL[noprop::sample_usize_in(ctx, 0..PATH_SPECIAL.len())] as char
        }
    }
}

/// PBT 用のステータスコード生成
///
/// 既知のレンジ (100..=101 / 200..=206 / 300..=308 / 400..=451 / 500..=511) から
/// 一様に選ぶ。
pub(crate) fn status_code(ctx: &mut noprop::TestCaseContext) -> u16 {
    match noprop::sample_usize_in(ctx, 0..5) {
        0 => noprop::sample_usize_in(ctx, 100..=101) as u16,
        1 => noprop::sample_usize_in(ctx, 200..=206) as u16,
        2 => noprop::sample_usize_in(ctx, 300..=308) as u16,
        3 => noprop::sample_usize_in(ctx, 400..=451) as u16,
        _ => noprop::sample_usize_in(ctx, 500..=511) as u16,
    }
}

/// PBT 用の reason phrase 生成
pub(crate) fn reason_phrase(ctx: &mut noprop::TestCaseContext) -> String {
    match noprop::sample_usize_in(ctx, 0..4) {
        0 => "OK".to_string(),
        1 => "Not Found".to_string(),
        2 => "Internal Server Error".to_string(),
        _ => {
            // [A-Za-z ]{1,32}
            let len = noprop::sample_usize_in(ctx, 1..=32);
            let mut s = String::with_capacity(len);
            for _ in 0..len {
                match noprop::sample_usize_in(ctx, 0..2) {
                    0 => s.push(char::from(b'A' + noprop::sample_usize_in(ctx, 0..26) as u8)),
                    _ => s.push(' '),
                }
            }
            s
        }
    }
}

/// PBT 用のボディデータ生成
pub(crate) fn body(ctx: &mut noprop::TestCaseContext) -> Vec<u8> {
    let len = noprop::sample_usize_in(ctx, 0..256);
    noprop::sample_bytes_vec(ctx, len)
}

/// 無効なヘッダー名の文字を生成する
/// 注: `:` はヘッダーの区切り文字として解釈されるため除外
pub(crate) fn invalid_header_name_char(ctx: &mut noprop::TestCaseContext) -> char {
    const CHARS: &[char] = &[
        '@', '[', ']', '\\', '{', '}', '<', '>', '(', ')', ',', ';', '"', '/', '?', '=',
    ];
    CHARS[noprop::sample_usize_in(ctx, 0..CHARS.len())]
}

/// 有効なヘッダー名の文字を生成する
pub(crate) fn valid_header_name_special_char(ctx: &mut noprop::TestCaseContext) -> char {
    const CHARS: &[char] = &[
        '!', '#', '$', '%', '&', '\'', '*', '+', '^', '`', '|', '~', '-', '_', '.',
    ];
    CHARS[noprop::sample_usize_in(ctx, 0..CHARS.len())]
}

/// Transfer-Encoding トークン生成
///
/// 固定トークン 5 値から一様に選ぶ。現在の prop_decoder 内のテストは
/// すべて "chunked" を除外して使用する (head.rs の
/// `non_chunked_transfer_encoding_token` が担う) ため直接の呼び出し元は
/// ないが、値集合の仕様を明示するために保持する。
#[expect(dead_code)]
pub(crate) fn transfer_encoding_token(ctx: &mut noprop::TestCaseContext) -> String {
    match noprop::sample_usize_in(ctx, 0..5) {
        0 => "chunked".to_string(),
        1 => "gzip".to_string(),
        2 => "deflate".to_string(),
        3 => "compress".to_string(),
        _ => "identity".to_string(),
    }
}
