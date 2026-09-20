use std::{ffi::c_void, ptr};

use windows::{
    core::Interface,
    Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap},
    Media::Ocr::OcrEngine,
    Storage::Streams::Buffer,
    Win32::{
        Graphics::Gdi::{
            CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
            ReleaseDC, SelectObject, SetStretchBltMode, StretchBlt, BITMAPINFO, BITMAPINFOHEADER,
            BI_RGB, CAPTUREBLT, DIB_RGB_COLORS, HALFTONE, HGDIOBJ, SRCCOPY,
        },
        System::WinRT::{IBufferByteAccess, RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
    },
};

use crate::errors::AppError;

pub fn capture_and_recognize(x: i32, y: i32, width: i32, height: i32) -> Result<String, AppError> {
    if width < 8 || height < 8 {
        return Err(AppError::Ocr("所选区域太小".into()));
    }

    unsafe { RoInitialize(RO_INIT_MULTITHREADED) }
        .map_err(|error| AppError::Ocr(format!("无法初始化 Windows OCR：{error}")))?;
    let result = recognize_region(x, y, width, height);
    unsafe { RoUninitialize() };
    result
}

fn recognize_region(x: i32, y: i32, width: i32, height: i32) -> Result<String, AppError> {
    let max_dimension = OcrEngine::MaxImageDimension()
        .map_err(|error| AppError::Ocr(format!("无法读取 OCR 图像限制：{error}")))?
        .min(i32::MAX as u32) as i32;
    let (output_width, output_height) = scaled_dimensions(width, height, max_dimension);
    let pixels = capture_bgra(x, y, width, height, output_width, output_height)?;

    let byte_count =
        u32::try_from(pixels.len()).map_err(|_| AppError::Ocr("所选区域像素数据过大".into()))?;
    let buffer = Buffer::Create(byte_count)
        .map_err(|error| AppError::Ocr(format!("无法创建图像缓冲区：{error}")))?;
    let access: IBufferByteAccess = buffer
        .cast()
        .map_err(|error| AppError::Ocr(format!("无法访问图像缓冲区：{error}")))?;
    let destination = unsafe { access.Buffer() }
        .map_err(|error| AppError::Ocr(format!("无法写入图像缓冲区：{error}")))?;
    unsafe { ptr::copy_nonoverlapping(pixels.as_ptr(), destination, pixels.len()) };
    buffer
        .SetLength(byte_count)
        .map_err(|error| AppError::Ocr(format!("无法提交图像缓冲区：{error}")))?;

    let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
        &buffer,
        BitmapPixelFormat::Bgra8,
        output_width,
        output_height,
    )
    .map_err(|error| AppError::Ocr(format!("无法创建 OCR 图像：{error}")))?;
    let engine = OcrEngine::TryCreateFromUserProfileLanguages().map_err(|error| {
        AppError::Ocr(format!("请在 Windows 中安装中文或英文 OCR 语言包：{error}"))
    })?;
    let result = engine
        .RecognizeAsync(&bitmap)
        .and_then(|operation| operation.get())
        .map_err(|error| AppError::Ocr(format!("Windows OCR 无法识别图像：{error}")))?;
    let text = result
        .Text()
        .map_err(|error| AppError::Ocr(format!("无法读取 OCR 结果：{error}")))?
        .to_string();
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err(AppError::OcrNoText);
    }
    Ok(text)
}

fn scaled_dimensions(width: i32, height: i32, max_dimension: i32) -> (i32, i32) {
    let scale = (max_dimension as f64 / width.max(height) as f64).min(1.0);
    (
        ((width as f64 * scale).round() as i32).max(1),
        ((height as f64 * scale).round() as i32).max(1),
    )
}

fn capture_bgra(
    x: i32,
    y: i32,
    source_width: i32,
    source_height: i32,
    output_width: i32,
    output_height: i32,
) -> Result<Vec<u8>, AppError> {
    let screen = unsafe { GetDC(None) };
    if screen.is_invalid() {
        return Err(AppError::Ocr("无法读取屏幕画面".into()));
    }
    let memory = unsafe { CreateCompatibleDC(Some(screen)) };
    if memory.is_invalid() {
        unsafe { ReleaseDC(None, screen) };
        return Err(AppError::Ocr("无法创建屏幕捕获缓冲区".into()));
    }
    let bitmap = unsafe { CreateCompatibleBitmap(screen, output_width, output_height) };
    if bitmap.is_invalid() {
        unsafe {
            let _ = DeleteDC(memory);
            ReleaseDC(None, screen);
        }
        return Err(AppError::Ocr("无法创建屏幕位图".into()));
    }
    let previous = unsafe { SelectObject(memory, HGDIOBJ(bitmap.0)) };

    let result = (|| -> Result<Vec<u8>, AppError> {
        unsafe { SetStretchBltMode(memory, HALFTONE) };
        let raster = windows::Win32::Graphics::Gdi::ROP_CODE(SRCCOPY.0 | CAPTUREBLT.0);
        if !unsafe {
            StretchBlt(
                memory,
                0,
                0,
                output_width,
                output_height,
                Some(screen),
                x,
                y,
                source_width,
                source_height,
                raster,
            )
        }
        .as_bool()
        {
            return Err(AppError::Ocr("截取屏幕区域失败".into()));
        }

        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: output_width,
                biHeight: -output_height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let byte_count = (output_width as usize)
            .checked_mul(output_height as usize)
            .and_then(|value| value.checked_mul(4))
            .ok_or_else(|| AppError::Ocr("所选区域像素数据过大".into()))?;
        let mut pixels = vec![0u8; byte_count];
        let scan_lines = unsafe {
            GetDIBits(
                memory,
                bitmap,
                0,
                output_height as u32,
                Some(pixels.as_mut_ptr().cast::<c_void>()),
                &mut info,
                DIB_RGB_COLORS,
            )
        };
        if scan_lines != output_height {
            return Err(AppError::Ocr("读取屏幕像素失败".into()));
        }
        Ok(pixels)
    })();

    unsafe {
        let _ = SelectObject(memory, previous);
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        let _ = DeleteDC(memory);
        ReleaseDC(None, screen);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::scaled_dimensions;

    #[test]
    fn keeps_small_regions_and_scales_large_regions_proportionally() {
        assert_eq!(scaled_dimensions(800, 600, 2_600), (800, 600));
        assert_eq!(scaled_dimensions(3_840, 2_160, 2_600), (2_600, 1_463));
    }
}
