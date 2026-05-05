//! デコーダーの内部バッファ操作の共通ヘルパー
//!
//! `RequestDecoder` と `ResponseDecoder` で完全に同一だったバッファ操作
//! (`feed` / `feed_unchecked` / `mut_buf` / `advance_buf` / `available_buf`
//! / `remaining`) の実装を集約し、将来のバグ修正時に片側だけ修正される
//! リスクを排除する。

use crate::error::Error;
use alloc::vec::Vec;

/// `&[u8]` 由来のバイト列を内部バッファ末尾に追加する
///
/// `data` を `extend_from_slice` でコピーする (1 回の memcpy)。
///
/// # Errors
///
/// `buf.len() + data.len()` が `max` を超える場合は `BufferOverflow` を返し、
/// バッファは変更しない。
///
/// # Panics (debug only)
///
/// `pending != 0` の状態で呼ばれた場合 (= 直前の `mut_buf` の枠が未確定の
/// まま `feed` が呼ばれた場合)、debug ビルドでは panic する。release では
/// 何もチェックせず、未確定領域の後ろに `data` が追記されることになる。
pub(super) fn feed(
    buf: &mut Vec<u8>,
    pending: usize,
    max: usize,
    data: &[u8],
) -> Result<(), Error> {
    debug_assert!(pending == 0, "feed called with pending mut_buf");
    let new_size = buf.len() + data.len();
    if new_size > max {
        return Err(Error::BufferOverflow {
            size: new_size,
            limit: max,
        });
    }
    buf.extend_from_slice(data);
    Ok(())
}

/// `feed` の `max_buffer_size` チェックを省いた版
///
/// # Panics (debug only)
///
/// `feed` と同様、`pending != 0` の状態で呼ばれた場合 debug ビルドでは panic する。
pub(super) fn feed_unchecked(buf: &mut Vec<u8>, pending: usize, data: &[u8]) {
    debug_assert!(pending == 0, "feed_unchecked called with pending mut_buf");
    buf.extend_from_slice(data);
}

/// 内部バッファ末尾に `len` バイトの書き込み枠を確保し、ゼロ初期化された可変
/// スライスを返す
///
/// 返るスライスは `Vec::resize(_, 0)` によりゼロ初期化済みなので、
/// `std::io::Read::read` 等にそのまま渡せる。書き込み後は必ず `advance_buf` で
/// 実書き込みバイト数を通知する必要がある。
///
/// # Pending 領域の扱い
///
/// 関数の先頭で必ず `pending` 領域 (= 直前の `mut_buf` で確保された未確定領域) を
/// 破棄してから動作する (`advance_buf` 呼び忘れの回復が目的)。したがって:
///
/// - 成功時: pending 領域破棄 → 新規枠を確保 → `*pending = len` に更新
/// - エラー時 (`BufferOverflow`): pending 領域破棄 → 新規枠は確保せず `*pending = 0`
///
/// エラー時に「呼び出し前の状態に巻き戻る」のではなく、「pending 領域は破棄
/// された上で新規枠が確保されない」状態になる点に注意。
///
/// # Errors
///
/// pending 破棄後の `buf.len() + len` が `max` を超える場合は `BufferOverflow`
/// を返す。
pub(super) fn mut_buf<'a>(
    buf: &'a mut Vec<u8>,
    pending: &mut usize,
    max: usize,
    len: usize,
) -> Result<&'a mut [u8], Error> {
    if *pending > 0 {
        let new_len = buf.len() - *pending;
        buf.truncate(new_len);
        *pending = 0;
    }

    let new_size = buf.len() + len;
    if new_size > max {
        return Err(Error::BufferOverflow {
            size: new_size,
            limit: max,
        });
    }

    let old = buf.len();
    buf.resize(new_size, 0);
    *pending = len;
    Ok(&mut buf[old..])
}

/// 直前の `mut_buf` で確保した枠のうち、実際に書き込まれた `len` バイトを確定する
///
/// 残り (`mut_buf` で確保した長さ - `len`) は破棄される。
/// `len = 0` で呼ぶと枠全体が破棄される (EOF や read 失敗時のリセット用途)。
///
/// `len > *pending` の場合、debug ビルドでは panic、release ビルドでは
/// `*pending` で飽和する。
pub(super) fn advance_buf(buf: &mut Vec<u8>, pending: &mut usize, len: usize) {
    debug_assert!(len <= *pending, "advance_buf len exceeds pending");
    let len = len.min(*pending);
    let drop = *pending - len;
    if drop > 0 {
        let new_len = buf.len() - drop;
        buf.truncate(new_len);
    }
    *pending = 0;
}

/// 書き込み可能な残り容量を返す
///
/// `max` から現在のバッファ長 (確定済みデータ + 未確定 `pending`) を引いた値。
pub(super) fn available_buf(buf: &[u8], max: usize) -> usize {
    max.saturating_sub(buf.len())
}

/// バッファの確定済みデータを参照する
///
/// # Panics (debug only)
///
/// `pending != 0` の状態で呼ばれた場合 (= `mut_buf` で確保した枠が未確定の
/// まま `remaining` が呼ばれた場合)、debug ビルドでは panic する。release では
/// 未確定領域 (ゼロ初期化されたバイト) も含めて返してしまうため呼び出し側
/// の責任で `pending == 0` の状態で呼ぶこと。
pub(super) fn remaining(buf: &[u8], pending: usize) -> &[u8] {
    debug_assert!(pending == 0, "remaining called with pending mut_buf");
    buf
}
