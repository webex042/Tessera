//! Private clipboard insertion for drafts (plan §12.3).
//!
//! [`set_private_text`] saves the current clipboard text, then puts the draft on the clipboard
//! together with three registered formats that keep it out of clipboard history (Win+V) **and**
//! cloud clipboard sync:
//! - `ExcludeClipboardContentFromMonitorProcessing` (any data),
//! - `CanIncludeInClipboardHistory` = DWORD 0,
//! - `CanUploadToCloudClipboard` = DWORD 0 (history exclusion alone does not stop upload).
//!
//! [`restore`] puts the saved contents back (also excluded from history), but only if nothing
//! else changed the clipboard meanwhile (sequence number). Every memory-based format is saved
//! (plain text, HTML, RTF, DIB images, file lists, app formats). Formats held as GDI handles
//! (bitmaps, metafiles) are skipped when Windows can synthesise them from a saved format;
//! otherwise, or above [`MAX_SAVE_BYTES`], [`set_private_text`] refuses
//! ([`ClipError::NotText`]) and the caller offers Copy instead. Saved data is wiped after use.

use std::time::{Duration, Instant};

use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, GetClipboardSequenceNumber,
    IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY, VK_CONTROL,
};
use windows::core::w;
use zeroize::Zeroizing;

const CF_UNICODETEXT: u32 = 13;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ClipError {
    #[error("clipboard busy")]
    Busy,
    /// The clipboard holds non-text data that could not be put back afterwards.
    #[error("clipboard holds non-text data")]
    NotText,
    #[error("clipboard call {0} failed")]
    Win(&'static str),
}

/// Saving more than this would hold too much of the user's data in memory: refuse instead.
pub const MAX_SAVE_BYTES: usize = 64 << 20;

/// Saved clipboard contents: (format, data) in clipboard order.
type Saved = Vec<(u32, Zeroizing<Vec<u8>>)>;

/// What [`restore`] needs: the previous contents and our clipboard sequence number.
pub struct ClipGuard {
    prev: Saved,
    seq: u32,
}

const CF_BITMAP: u32 = 2;
const CF_METAFILEPICT: u32 = 3;
const CF_PALETTE: u32 = 9;
const CF_DIB: u32 = 8;
const CF_ENHMETAFILE: u32 = 14;
const CF_DIBV5: u32 = 17;
/// Formats whose data is a GDI handle, not memory.
const HANDLE_FORMATS: [u32; 8] = [
    CF_BITMAP,
    CF_METAFILEPICT,
    CF_PALETTE,
    CF_ENHMETAFILE,
    0x80,
    0x82,
    0x83,
    0x8E,
];

struct Open;

impl Open {
    fn new() -> Result<Self, ClipError> {
        // Another app may hold the clipboard for a moment; retry briefly.
        let end = Instant::now() + Duration::from_millis(500);
        loop {
            // SAFETY: plain call; balanced by CloseClipboard in Drop.
            if unsafe { OpenClipboard(None) }.is_ok() {
                return Ok(Open);
            }
            if Instant::now() > end {
                return Err(ClipError::Busy);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        // SAFETY: this thread opened the clipboard in `Open::new`.
        let _ = unsafe { CloseClipboard() };
    }
}

fn private_formats() -> [u32; 3] {
    // SAFETY: registering a format name is idempotent and has no other effect.
    unsafe {
        [
            RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing")),
            RegisterClipboardFormatW(w!("CanIncludeInClipboardHistory")),
            RegisterClipboardFormatW(w!("CanUploadToCloudClipboard")),
        ]
    }
}

fn global_from(bytes: &[u8]) -> Result<HGLOBAL, ClipError> {
    // SAFETY: allocate, copy `bytes` into the locked block, unlock. On failure the block is freed.
    unsafe {
        let h = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)).map_err(|_| ClipError::Win("GlobalAlloc"))?;
        let p = GlobalLock(h) as *mut u8;
        if p.is_null() {
            let _ = GlobalFree(Some(h));
            return Err(ClipError::Win("GlobalLock"));
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
        let _ = GlobalUnlock(h);
        Ok(h)
    }
}

fn read_global(fmt: u32) -> Option<Zeroizing<Vec<u8>>> {
    // SAFETY: the clipboard is open (caller); the handle stays valid until it is closed; the copy
    // stays within GlobalSize.
    unsafe {
        let h = GetClipboardData(fmt).ok()?;
        let g = HGLOBAL(h.0);
        let p = GlobalLock(g) as *const u8;
        if p.is_null() {
            return None;
        }
        let v = Zeroizing::new(std::slice::from_raw_parts(p, GlobalSize(g)).to_vec());
        let _ = GlobalUnlock(g);
        Some(v)
    }
}

fn read_unicode() -> Option<Zeroizing<Vec<u16>>> {
    let b = read_global(CF_UNICODETEXT)?;
    let mut v: Zeroizing<Vec<u16>> =
        Zeroizing::new(b.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect());
    if let Some(n) = v.iter().position(|u| *u == 0) {
        v.truncate(n);
    }
    Some(v)
}

/// Everything on the clipboard that can be put back faithfully, or `NotText`.
fn save_all() -> Result<Saved, ClipError> {
    let private = private_formats();
    let mut formats = Vec::new();
    // SAFETY: the clipboard is open (caller); enumeration only reads format ids.
    unsafe {
        let mut f = 0u32;
        loop {
            f = EnumClipboardFormats(f);
            if f == 0 {
                break;
            }
            formats.push(f);
        }
    }
    let has_dib = formats.contains(&CF_DIB) || formats.contains(&CF_DIBV5);
    let mut out = Vec::new();
    let mut total = 0usize;
    for f in formats {
        if private.contains(&f) {
            continue;
        }
        if HANDLE_FORMATS.contains(&f) {
            // A bitmap is synthesised again from the saved DIB; anything else cannot be kept.
            if (f == CF_BITMAP || f == CF_PALETTE) && has_dib {
                continue;
            }
            return Err(ClipError::NotText);
        }
        let Some(data) = read_global(f) else { continue };
        total += data.len();
        if total > MAX_SAVE_BYTES {
            return Err(ClipError::NotText);
        }
        out.push((f, data));
    }
    Ok(out)
}

fn set_raw(fmt: u32, data: &[u8]) -> Result<(), ClipError> {
    let h = global_from(data)?;
    // SAFETY: the clipboard is open and owned by this thread; the system owns `h` on success.
    unsafe {
        if SetClipboardData(fmt, Some(HANDLE(h.0))).is_err() {
            let _ = GlobalFree(Some(h));
            return Err(ClipError::Win("SetClipboardData"));
        }
    }
    Ok(())
}

fn set_private_markers() -> Result<(), ClipError> {
    let [exclude, history, cloud] = private_formats();
    set_raw(exclude, &[0u8; 4])?;
    set_raw(history, &0u32.to_le_bytes())?;
    set_raw(cloud, &0u32.to_le_bytes())
}

fn put(text: &[u16]) -> Result<(), ClipError> {
    let mut bytes: Zeroizing<Vec<u8>> = Zeroizing::new(Vec::with_capacity(text.len() * 2 + 2));
    for u in text.iter().chain(std::iter::once(&0u16)) {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    set_raw(CF_UNICODETEXT, &bytes)?;
    set_private_markers()
}

/// Put `text` on the clipboard, privately; returns what [`restore`] needs.
pub fn set_private_text(text: &str) -> Result<ClipGuard, ClipError> {
    let prev = {
        let _open = Open::new()?;
        let prev = save_all()?;
        let utf16: Zeroizing<Vec<u16>> = Zeroizing::new(text.encode_utf16().collect());
        // SAFETY: the clipboard is open.
        unsafe { EmptyClipboard() }.map_err(|_| ClipError::Win("EmptyClipboard"))?;
        put(&utf16)?;
        prev
    };
    // Read after closing: closing the clipboard itself advances the sequence number, so a
    // value taken while it is open never matches later and nothing would be restored.
    // SAFETY: plain query.
    let seq = unsafe { GetClipboardSequenceNumber() };
    Ok(ClipGuard { prev, seq })
}

/// Put the previous text back if the clipboard still holds ours. Returns whether it did.
pub fn restore(g: ClipGuard) -> Result<bool, ClipError> {
    // SAFETY: plain query.
    if unsafe { GetClipboardSequenceNumber() } != g.seq {
        return Ok(false); // the user or another app copied something since: leave it
    }
    let _open = Open::new()?;
    // SAFETY: the clipboard is open.
    unsafe { EmptyClipboard() }.map_err(|_| ClipError::Win("EmptyClipboard"))?;
    if !g.prev.is_empty() {
        for (f, data) in &g.prev {
            set_raw(*f, data)?;
        }
        set_private_markers()?;
    }
    Ok(true)
}

/// Ctrl+V into whatever has keyboard focus (the caller verified the target first).
pub fn send_paste() -> Result<(), ClipError> {
    let key = |vk: VIRTUAL_KEY, up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let v = VIRTUAL_KEY(b'V' as u16);
    let inputs = [
        key(VK_CONTROL, false),
        key(v, false),
        key(v, true),
        key(VK_CONTROL, true),
    ];
    // SAFETY: a fixed array of well-formed keyboard inputs.
    let n = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
    if n as usize == inputs.len() {
        Ok(())
    } else {
        Err(ClipError::Win("SendInput"))
    }
}

/// For tests: the current clipboard text and whether the three private formats are present.
/// Only the text format is read.
pub fn inspect() -> Result<(Option<String>, bool), ClipError> {
    let _open = Open::new()?;
    let text = read_unicode().map(|v| String::from_utf16_lossy(&v));
    // SAFETY: plain queries while the clipboard is open.
    let private = private_formats()
        .iter()
        .all(|f| unsafe { IsClipboardFormatAvailable(*f) }.is_ok());
    Ok((text, private))
}

/// For tests: every saved-able format on the clipboard with its data size (no contents), in
/// order, without Tessara's private markers and without formats Windows synthesises.
pub fn snapshot() -> Result<Vec<(u32, usize)>, ClipError> {
    let _open = Open::new()?;
    Ok(save_all()?.into_iter().map(|(f, d)| (f, d.len())).collect())
}

/// For tests only: put the saved contents back even if the clipboard changed since (a test that
/// deliberately overwrote it must still return the user's data).
pub fn restore_unconditionally(g: ClipGuard) -> Result<(), ClipError> {
    let _open = Open::new()?;
    // SAFETY: the clipboard is open.
    unsafe { EmptyClipboard() }.map_err(|_| ClipError::Win("EmptyClipboard"))?;
    for (f, data) in &g.prev {
        set_raw(*f, data)?;
    }
    if !g.prev.is_empty() {
        set_private_markers()?;
    }
    Ok(())
}

/// For tests only: put text on the clipboard the ordinary way (history and cloud allowed), as
/// a positive control for the exclusion check.
pub fn set_ordinary_text_for_test(text: &str) -> Result<(), ClipError> {
    let _open = Open::new()?;
    // SAFETY: the clipboard is open.
    unsafe { EmptyClipboard() }.map_err(|_| ClipError::Win("EmptyClipboard"))?;
    let mut bytes: Zeroizing<Vec<u8>> = Zeroizing::new(Vec::new());
    for u in text.encode_utf16().chain(std::iter::once(0)) {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    set_raw(CF_UNICODETEXT, &bytes)
}
