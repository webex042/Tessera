//! Rung 2: one Windows Graphics Capture frame + Windows OCR, with a wall-clock deadline
//! (plan §5.4 rung 2, §19 "WGC frame + OCR ≤ 500 ms p95").
//!
//! The frame exists only in GPU/CPU memory; its pixel buffer is zeroed before release and nothing
//! is written to disk. Like rung 1, all calls run on one dedicated thread ([`DeadlineWorker`]).

use std::time::{Duration, Instant};

use tsr_core::Sensitive;
use windows::Graphics::Capture::{
    Direct3D11CaptureFramePool, GraphicsCaptureAccess, GraphicsCaptureAccessKind, GraphicsCaptureItem,
};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::Imaging::{BitmapBufferAccessMode, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Security::Authorization::AppCapabilityAccess::AppCapabilityAccessStatus;
use windows::Win32::Foundation::{E_ABORT, E_INVALIDARG, HMODULE, HWND};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device,
};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::System::WinRT::Direct3D11::CreateDirect3D11DeviceFromDXGIDevice;
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::Win32::System::WinRT::IMemoryBufferByteAccess;
use windows::core::Interface;

use crate::deadline::{DeadlineError, DeadlineWorker};

pub const DEFAULT_DEADLINE: Duration = Duration::from_millis(485);

/// OCR output. Text is content.
#[derive(Debug)]
pub struct OcrText {
    pub text: Sensitive<String>,
    pub width: u32,
    pub height: u32,
    pub capture_ms: u32,
    pub ocr_ms: u32,
    pub border_free: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OcrError {
    #[error(transparent)]
    Deadline(#[from] DeadlineError),
    #[error("capture/OCR error {0:#010x}")]
    Com(i32),
    #[error("window larger than the OCR engine accepts")]
    TooLarge,
    #[error("capture/OCR not available on this system")]
    Unavailable,
}

impl From<windows::core::Error> for OcrError {
    fn from(e: windows::core::Error) -> Self {
        OcrError::Com(e.code().0)
    }
}

struct State {
    device: Option<IDirect3DDevice>,
    ocr: Option<OcrEngine>,
    max_dim: u32,
}

pub struct OcrReader {
    worker: DeadlineWorker<isize, Result<OcrText, OcrError>>,
    pub deadline: Duration,
    /// Whether Windows granted border-free capture (no yellow border; plan §5.4).
    pub borderless: bool,
}

fn make_device() -> windows::core::Result<IDirect3DDevice> {
    let mut d3d: Option<ID3D11Device> = None;
    // SAFETY: standard D3D11 device creation with an out-param.
    unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut d3d),
            None,
            None,
        )?;
    }
    let dxgi: IDXGIDevice = d3d.ok_or_else(|| windows::core::Error::from_hresult(E_ABORT))?.cast()?;
    // SAFETY: wraps a live DXGI device for WinRT use.
    unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi)? }.cast()
}

impl OcrReader {
    pub fn new() -> std::io::Result<Self> {
        let borderless = GraphicsCaptureAccess::RequestAccessAsync(GraphicsCaptureAccessKind::Borderless)
            .and_then(|op| op.join())
            .is_ok_and(|s| s == AppCapabilityAccessStatus::Allowed);
        let worker = DeadlineWorker::spawn(
            "tsr-ocr",
            || {
                let ocr = OcrEngine::TryCreateFromUserProfileLanguages().ok();
                let max_dim = OcrEngine::MaxImageDimension().unwrap_or(0);
                State {
                    device: make_device().ok(),
                    ocr,
                    max_dim,
                }
            },
            |st: &mut State, hwnd: isize| capture_and_ocr(st, HWND(hwnd as *mut _)),
        )?;
        Ok(OcrReader {
            worker,
            deadline: DEFAULT_DEADLINE,
            borderless,
        })
    }

    pub fn read(&self, hwnd: HWND) -> Result<OcrText, OcrError> {
        self.worker.call(hwnd.0 as isize, self.deadline)?
    }

    pub fn wait_idle(&self, max: Duration) -> Option<Duration> {
        self.worker.wait_idle(max)
    }
}

fn capture_and_ocr(st: &State, hwnd: HWND) -> Result<OcrText, OcrError> {
    let (Some(device), Some(engine)) = (&st.device, &st.ocr) else {
        return Err(OcrError::Unavailable);
    };
    let t0 = Instant::now();
    let interop = windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
    // SAFETY: hwnd is a top-level window handle supplied by the capture loop.
    let item: GraphicsCaptureItem = unsafe { interop.CreateForWindow(hwnd)? };
    let size = item.Size()?;
    let pool =
        Direct3D11CaptureFramePool::CreateFreeThreaded(device, DirectXPixelFormat::B8G8R8A8UIntNormalized, 1, size)?;
    let session = pool.CreateCaptureSession(&item)?;
    let _ = session.SetIsCursorCaptureEnabled(false);
    let border_free = session.SetIsBorderRequired(false).is_ok();
    session.StartCapture()?;
    let deadline = Instant::now() + Duration::from_millis(400);
    let frame = loop {
        if let Ok(f) = pool.TryGetNextFrame() {
            break f;
        }
        if Instant::now() > deadline {
            let _ = session.Close();
            let _ = pool.Close();
            return Err(OcrError::Com(E_ABORT.0));
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let bmp = SoftwareBitmap::CreateCopyFromSurfaceAsync(&frame.Surface()?)?.join()?;
    let _ = frame.Close();
    let _ = session.Close();
    let _ = pool.Close();
    let capture_ms = t0.elapsed().as_millis() as u32;

    let (w, h) = (bmp.PixelWidth()? as u32, bmp.PixelHeight()? as u32);
    let t1 = Instant::now();
    let result = if w > st.max_dim || h > st.max_dim {
        Err(OcrError::TooLarge) // Stage 2: downscale instead (§5.4 step 2)
    } else {
        engine
            .RecognizeAsync(&bmp)
            .and_then(|op| op.join())
            .map_err(OcrError::from)
            .and_then(|r| {
                let mut out = String::new();
                for line in r.Lines()? {
                    out.push_str(&line.Text()?.to_string());
                    out.push('\n');
                }
                Ok(out)
            })
    };
    let ocr_ms = t1.elapsed().as_millis() as u32;
    wipe(&bmp);
    let _ = bmp.Close();
    let text = result?;
    if w == 0 || h == 0 {
        return Err(OcrError::Com(E_INVALIDARG.0));
    }
    Ok(OcrText {
        text: Sensitive::new(text),
        width: w,
        height: h,
        capture_ms,
        ocr_ms,
        border_free,
    })
}

/// Zero the bitmap's pixels before the buffer is released (plan §5.4 step 6).
fn wipe(bmp: &SoftwareBitmap) {
    let Ok(buf) = bmp.LockBuffer(BitmapBufferAccessMode::Write) else {
        return;
    };
    if let Ok(reference) = buf.CreateReference()
        && let Ok(access) = reference.cast::<IMemoryBufferByteAccess>()
    {
        let mut ptr = std::ptr::null_mut();
        let mut len = 0u32;
        // SAFETY: GetBuffer returns the locked buffer's base and length while `reference` lives.
        unsafe {
            if access.GetBuffer(&mut ptr, &mut len).is_ok() && !ptr.is_null() {
                std::ptr::write_bytes(ptr, 0, len as usize);
            }
        }
        let _ = reference.Close();
    }
    let _ = buf.Close();
}

pub use crate::ocr_guard::looks_like_password_screen;

#[cfg(test)]
mod tests {
    use super::*;

    /// REAL: capture + OCR the taskbar within the deadline.
    #[test]
    fn real_ocr_of_the_taskbar_respects_the_deadline() {
        use windows::Win32::UI::WindowsAndMessaging::FindWindowW;
        use windows::core::w;
        let Ok(tray) = (unsafe { FindWindowW(w!("Shell_TrayWnd"), None) }) else {
            return;
        };
        let r = OcrReader::new().unwrap();
        let t0 = Instant::now();
        let out = r.read(tray);
        let took = t0.elapsed();
        println!(
            "REAL taskbar OCR: {:?} in {took:?}, borderless={}",
            out.as_ref().map(|o| (o.width, o.height, o.capture_ms, o.ocr_ms)),
            r.borderless
        );
        assert!(took <= Duration::from_millis(500), "{took:?}");
    }
}
