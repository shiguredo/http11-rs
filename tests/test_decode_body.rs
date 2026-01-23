//! ボディデコードのテスト
//!
//! 不完全なボディ（接続切断シナリオ）が正しく検出されることを確認する。
//!
//! ## なぜ PBT (Property-Based Testing) ではテストできないのか
//!
//! このテストは PBT ではカバーできない領域をテストしている。その理由は以下の通り。
//!
//! ### 1. PBT がテストするもの
//!
//! PBT は「デコーダーの正しさ」をテストする。具体的には：
//! - 任意の有効な HTTP メッセージをエンコードし、デコードすると元に戻る（ラウンドトリップ）
//! - 様々なエッジケース（大きなボディ、特殊文字、複数チャンクなど）で正しく動作する
//!
//! PBT は「完全なデータが与えられた場合の正しさ」を検証する。
//!
//! ### 2. このテストがテストするもの
//!
//! このテストは「不完全なデータが与えられた場合のデコーダーの状態」をテストする。
//! 具体的には：
//! - Content-Length で宣言されたバイト数より少ないデータしか受信できなかった場合
//! - Chunked エンコーディングで終端チャンク (`0\r\n\r\n`) を受信する前に接続が切れた場合
//!
//! これらは「ネットワーク I/O の途中で接続が切断された」というシナリオに対応する。
//!
//! ### 3. PBT で生成できないデータ
//!
//! PBT のデータ生成器は「有効な HTTP メッセージ」を生成する。
//! しかし、接続切断シナリオでは「途中で切れた不完全なデータ」が必要になる。
//! このようなデータは PBT の生成器では自然に生成されない。
//!
//! 仮に不完全なデータを生成できたとしても、PBT は「プロパティ（性質）」を検証する。
//! 不完全なデータに対するプロパティは「Complete に到達しないこと」だが、
//! これは単純なアサーションで十分であり、PBT の強みである「多様な入力での検証」は活きない。
//!
//! ### 4. アプリケーションコードの責務
//!
//! デコーダー自体は正しく動作する。不完全なデータを与えると、正しく `Continue` を返す。
//! 問題は「アプリケーションコードがその状態を正しく処理するか」という点である。
//!
//! 例えば reverse proxy の場合：
//! - クライアントが切断したのに、部分的なリクエストボディを upstream に送信してしまう
//! - upstream が切断したのに、不完全なレスポンスを downstream に完了として送信してしまう
//!
//! これらは「デコーダーの正しさ」ではなく「アプリケーションの正しさ」の問題である。
//! PBT はデコーダーをテストするが、アプリケーションの使い方までは検証できない。
//!
//! ### 5. このテストの価値
//!
//! このテストは、デコーダーを使うアプリケーションが参照すべき「期待される動作」を示す。
//! アプリケーション開発者は、このテストを見て：
//! - 不完全なボディでは `Complete` に到達しないこと
//! - `peek_body()` が `None` を返しても `progress()` で状態遷移を試みる必要があること
//! - ループを抜けた後に `Complete` に到達したかを確認する必要があること
//!
//! を理解できる。

use shiguredo_http11::{BodyKind, BodyProgress, RequestDecoder, ResponseDecoder};

/// 不完全な Content-Length リクエストボディのテスト
///
/// クライアントが途中で切断した場合、Complete に到達しないことを確認する。
#[test]
fn incomplete_content_length_body() {
    let mut decoder = RequestDecoder::new();
    decoder
        .feed(b"POST / HTTP/1.1\r\nHost: example.com\r\nContent-Length: 100\r\n\r\n")
        .unwrap();
    decoder.feed(&[0u8; 50]).unwrap(); // 100 バイト中 50 バイトのみ

    let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
    assert!(matches!(body_kind, BodyKind::ContentLength(100)));

    // ボディを読み取っても Complete にならない
    let mut body = Vec::new();
    loop {
        if let Some(data) = decoder.peek_body() {
            body.extend_from_slice(data);
            let len = data.len();
            match decoder.consume_body(len).unwrap() {
                BodyProgress::Complete { .. } => panic!("should not complete with incomplete body"),
                BodyProgress::Continue => {}
            }
        } else {
            match decoder.progress().unwrap() {
                BodyProgress::Complete { .. } => panic!("should not complete with incomplete body"),
                BodyProgress::Continue => break, // データ不足
            }
        }
    }
    assert_eq!(body.len(), 50); // 50 バイトのみ読み取れた
}

/// 不完全な Chunked レスポンスボディのテスト
///
/// upstream が途中で切断した場合（終端チャンクがない）、Complete に到達しないことを確認する。
#[test]
fn incomplete_chunked_body() {
    let mut decoder = ResponseDecoder::new();
    decoder
        .feed(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n")
        .unwrap();
    decoder.feed(b"5\r\nhello\r\n").unwrap(); // 終端チャンク "0\r\n\r\n" がない

    let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
    assert!(matches!(body_kind, BodyKind::Chunked));

    // ボディを読み取っても Complete にならない
    let mut body = Vec::new();
    loop {
        if let Some(data) = decoder.peek_body() {
            body.extend_from_slice(data);
            let len = data.len();
            match decoder.consume_body(len).unwrap() {
                BodyProgress::Complete { .. } => {
                    panic!("should not complete without terminating chunk")
                }
                BodyProgress::Continue => {}
            }
        } else {
            // peek_body() が None でも progress() で状態遷移を試みる
            let remaining_before = decoder.remaining().len();
            match decoder.progress().unwrap() {
                BodyProgress::Complete { .. } => {
                    panic!("should not complete without terminating chunk")
                }
                BodyProgress::Continue => {
                    // remaining が変化した場合は継続、変化なしならデータ不足
                    if decoder.remaining().len() == remaining_before {
                        break;
                    }
                }
            }
        }
    }
    assert_eq!(body, b"hello");
}

/// 完全な Content-Length リクエストボディのテスト（正常系）
#[test]
fn complete_content_length_body() {
    let mut decoder = RequestDecoder::new();
    decoder
        .feed(b"POST / HTTP/1.1\r\nHost: example.com\r\nContent-Length: 5\r\n\r\nhello")
        .unwrap();

    let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
    assert!(matches!(body_kind, BodyKind::ContentLength(5)));

    let mut body = Vec::new();
    let mut completed = false;
    loop {
        if let Some(data) = decoder.peek_body() {
            body.extend_from_slice(data);
            let len = data.len();
            match decoder.consume_body(len).unwrap() {
                BodyProgress::Complete { .. } => {
                    completed = true;
                    break;
                }
                BodyProgress::Continue => {}
            }
        } else {
            match decoder.progress().unwrap() {
                BodyProgress::Complete { .. } => {
                    completed = true;
                    break;
                }
                BodyProgress::Continue => break,
            }
        }
    }
    assert!(completed);
    assert_eq!(body, b"hello");
}

/// 完全な Chunked レスポンスボディのテスト（正常系）
#[test]
fn complete_chunked_body() {
    let mut decoder = ResponseDecoder::new();
    decoder
        .feed(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\n\r\n")
        .unwrap();

    let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
    assert!(matches!(body_kind, BodyKind::Chunked));

    let mut body = Vec::new();
    let mut completed = false;
    loop {
        if let Some(data) = decoder.peek_body() {
            body.extend_from_slice(data);
            let len = data.len();
            match decoder.consume_body(len).unwrap() {
                BodyProgress::Complete { .. } => {
                    completed = true;
                    break;
                }
                BodyProgress::Continue => {}
            }
        } else {
            // peek_body() が None でも progress() で状態遷移を試みる
            let remaining_before = decoder.remaining().len();
            match decoder.progress().unwrap() {
                BodyProgress::Complete { .. } => {
                    completed = true;
                    break;
                }
                BodyProgress::Continue => {
                    // remaining が変化した場合は継続、変化なしならデータ不足
                    if decoder.remaining().len() == remaining_before {
                        break;
                    }
                }
            }
        }
    }
    assert!(completed);
    assert_eq!(body, b"hello");
}

/// close-delimited レスポンスの mark_eof テスト
///
/// upstream が接続を閉じた場合、mark_eof() を呼んで Complete に遷移することを確認する。
#[test]
fn close_delimited_mark_eof() {
    let mut decoder = ResponseDecoder::new();
    // Content-Length も Transfer-Encoding もない = close-delimited
    decoder.feed(b"HTTP/1.1 200 OK\r\n\r\n").unwrap();
    decoder.feed(b"hello world").unwrap();

    let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
    assert!(matches!(body_kind, BodyKind::CloseDelimited));

    // ボディを読み取る
    let mut body = Vec::new();
    while let Some(data) = decoder.peek_body() {
        body.extend_from_slice(data);
        let len = data.len();
        decoder.consume_body(len).unwrap();
    }

    // まだ Complete ではない (is_close_delimited() で確認)
    assert!(decoder.is_close_delimited());

    // EOF を通知
    decoder.mark_eof();

    // Complete に遷移 (is_close_delimited() が false になる)
    assert!(!decoder.is_close_delimited());
    assert_eq!(body, b"hello world");
}

/// close-delimited の decode() メソッドでの mark_eof テスト
#[test]
fn close_delimited_decode_with_mark_eof() {
    let mut decoder = ResponseDecoder::new();
    decoder.feed(b"HTTP/1.1 200 OK\r\n\r\nbody data").unwrap();

    // decode() はデータ不足で None を返す (close-delimited は mark_eof() が必要)
    assert!(decoder.decode().unwrap().is_none());
    assert!(decoder.is_close_delimited());

    // EOF を通知
    decoder.mark_eof();

    // 再度 decode() を呼ぶと Response が返る
    let response = decoder.decode().unwrap().unwrap();
    assert_eq!(response.body, b"body data");
}
