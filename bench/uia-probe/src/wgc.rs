//! Rung 2: one Windows Graphics Capture frame + Windows OCR (plan §5.4).
//!
//! The frame lives only in GPU/CPU memory and is dropped after OCR; nothing is written to disk.

use std::time::{Duration, Instant};

use windows::Graphics::Capture::{
    Direct3D11CaptureFramePool, GraphicsCaptureAccess, GraphicsCaptureAccessKind, GraphicsCaptureItem,
};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::Imaging::SoftwareBitmap;
use windows::Media::Ocr::OcrEngine;
use windows::Security::Authorization::AppCapabilityAccess::AppCapabilityAccessStatus;
use windows::Win32::Foundation::{HMODULE, HWND};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device,
};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::System::WinRT::Direct3D11::CreateDirect3D11DeviceFromDXGIDevice;
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::core::Interface;

#[derive(Debug, Default)]
pub struct OcrResult {
    pub capture_ms: u128,
    pub ocr_ms: u128,
    pub width: i32,
    pub height: i32,
    pub border_free: bool,
    pub text: String,
    pub error: Option<String>,
}

pub struct Capturer {
    device: IDirect3DDevice,
    ocr: OcrEngine,
    max_dim: u32,
    pub borderless_access: String,
}

impl Capturer {
    pub fn new() -> windows::core::Result<Self> {
        let mut d3d: Option<ID3D11Device> = None;
        // SAFETY: standard D3D11 device creation with out-param.
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
        let dxgi: IDXGIDevice = d3d.expect("device").cast()?;
        // SAFETY: wraps a live DXGI device for WinRT use.
        let device: IDirect3DDevice = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi)? }.cast()?;

        // Borderless capture needs consent on Windows 11; record what the OS says (§5.4 step 1).
        let borderless_access = match GraphicsCaptureAccess::RequestAccessAsync(GraphicsCaptureAccessKind::Borderless)
            .and_then(|op| op.join())
        {
            Ok(s) if s == AppCapabilityAccessStatus::Allowed => "allowed".into(),
            Ok(s) => format!("status_{}", s.0),
            Err(e) => format!("error_{:#010x}", e.code().0),
        };

        let ocr = OcrEngine::TryCreateFromUserProfileLanguages()?;
        let max_dim = OcrEngine::MaxImageDimension()?;
        Ok(Self {
            device,
            ocr,
            max_dim,
            borderless_access,
        })
    }

    pub fn capture_and_ocr(&self, hwnd: HWND) -> OcrResult {
        let mut r = OcrResult::default();
        let t0 = Instant::now();
        let bmp = match self.grab(hwnd, &mut r) {
            Ok(b) => b,
            Err(e) => {
                r.error = Some(format!("capture {:#010x}", e.code().0));
                return r;
            }
        };
        r.capture_ms = t0.elapsed().as_millis();

        let t1 = Instant::now();
        match self.ocr_bitmap(&bmp) {
            Ok(t) => r.text = t,
            Err(e) => r.error = Some(format!("ocr {:#010x}", e.code().0)),
        }
        r.ocr_ms = t1.elapsed().as_millis();
        let _ = bmp.Close();
        r
    }

    fn grab(&self, hwnd: HWND, r: &mut OcrResult) -> windows::core::Result<SoftwareBitmap> {
        let interop = windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
        // SAFETY: hwnd is a live top-level window from EnumWindows.
        let item: GraphicsCaptureItem = unsafe { interop.CreateForWindow(hwnd)? };
        let size = item.Size()?;
        r.width = size.Width;
        r.height = size.Height;
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &self.device,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            1,
            size,
        )?;
        let session = pool.CreateCaptureSession(&item)?;
        let _ = session.SetIsCursorCaptureEnabled(false);
        r.border_free = session.SetIsBorderRequired(false).is_ok();
        session.StartCapture()?;

        let deadline = Instant::now() + Duration::from_millis(1_000);
        let frame = loop {
            if let Ok(f) = pool.TryGetNextFrame() {
                break f;
            }
            if Instant::now() > deadline {
                let _ = session.Close();
                let _ = pool.Close();
                return Err(windows::core::Error::from_hresult(windows::Win32::Foundation::E_ABORT));
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        let bmp = SoftwareBitmap::CreateCopyFromSurfaceAsync(&frame.Surface()?)?.join()?;
        let _ = frame.Close();
        let _ = session.Close();
        let _ = pool.Close();
        Ok(bmp)
    }

    fn ocr_bitmap(&self, bmp: &SoftwareBitmap) -> windows::core::Result<String> {
        let (w, h) = (bmp.PixelWidth()? as u32, bmp.PixelHeight()? as u32);
        if w > self.max_dim || h > self.max_dim {
            // Stage 1 downscales (§5.4 step 2); the spike just reports it.
            return Err(windows::core::Error::from_hresult(
                windows::Win32::Foundation::E_INVALIDARG,
            ));
        }
        let result = self.ocr.RecognizeAsync(bmp)?.join()?;
        let mut out = String::new();
        for line in result.Lines()? {
            out.push_str(&line.Text()?.to_string());
            out.push('\n');
        }
        Ok(out)
    }
}
