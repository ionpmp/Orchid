//! Windows Imaging Component (WIC) HEIC/HEIF decode.
//!
//! Requires the OS HEIF Image Extensions (or equivalent codec). When the
//! codec is missing, callers surface [`crate::error::ViewerError::UnsupportedHeic`].

use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppRGBA, IWICFormatConverter,
    IWICImagingFactory, WICBitmapDitherTypeNone, WICBitmapPaletteTypeCustom,
    WICDecodeMetadataCacheOnDemand,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};

use crate::error::{Result, ViewerError};
use crate::image::loader::{ImageFormat, LoadedImage};

/// Decode HEIC/HEIF bytes via WIC into RGBA8.
pub(crate) fn decode_heic_wic(bytes: &[u8], size: u64) -> Result<LoadedImage> {
    decode_heic_wic_inner(bytes)
        .map(|(rgba, width, height)| LoadedImage {
            rgba: std::sync::Arc::new(rgba),
            width,
            height,
            format: ImageFormat::Heic,
            original_size_bytes: size,
        })
        .map_err(|e| {
            tracing::debug!(error = %e, "WIC HEIC decode failed");
            ViewerError::UnsupportedHeic
        })
}

fn decode_heic_wic_inner(bytes: &[u8]) -> windows::core::Result<(Vec<u8>, u32, u32)> {
    // Best-effort COM init on this blocking worker thread. Ignore "already
    // initialized" / apartment-mismatch — we only need a usable apartment.
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };

    unsafe {
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;

        let stream = factory.CreateStream()?;
        // Buffer must outlive the stream; `bytes` lives for this whole call.
        stream.InitializeFromMemory(bytes)?;

        let decoder = factory.CreateDecoderFromStream(
            &stream,
            std::ptr::null(),
            WICDecodeMetadataCacheOnDemand,
        )?;
        let frame = decoder.GetFrame(0)?;

        let converter: IWICFormatConverter = factory.CreateFormatConverter()?;
        converter.Initialize(
            &frame,
            &GUID_WICPixelFormat32bppRGBA,
            WICBitmapDitherTypeNone,
            None,
            0.0,
            WICBitmapPaletteTypeCustom,
        )?;

        let mut width = 0u32;
        let mut height = 0u32;
        converter.GetSize(&mut width, &mut height)?;
        if width == 0 || height == 0 {
            return Err(windows::core::Error::from(
                windows::Win32::Foundation::E_FAIL,
            ));
        }

        let stride = width.checked_mul(4).ok_or(windows::core::Error::from(
            windows::Win32::Foundation::E_OUTOFMEMORY,
        ))?;
        let buf_len = (stride as usize)
            .checked_mul(height as usize)
            .ok_or(windows::core::Error::from(
                windows::Win32::Foundation::E_OUTOFMEMORY,
            ))?;
        let mut rgba = vec![0u8; buf_len];
        converter.CopyPixels(std::ptr::null(), stride, &mut rgba)?;
        Ok((rgba, width, height))
    }
}
