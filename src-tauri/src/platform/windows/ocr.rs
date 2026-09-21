use std::{ffi::c_void, ptr};

use windows::{
    core::{Interface, HSTRING},
    Globalization::Language,
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

use image::{codecs::png::PngEncoder, ExtendedColorType, ImageEncoder};

use crate::{config::OcrLanguage, errors::AppError};

pub fn capture_and_recognize(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    language: OcrLanguage,
) -> Result<String, AppError> {
    if width < 8 || height < 8 {
        return Err(AppError::Ocr("所选区域太小".into()));
    }

    unsafe { RoInitialize(RO_INIT_MULTITHREADED) }
        .map_err(|error| AppError::Ocr(format!("无法初始化 Windows OCR：{error}")))?;
    let result = recognize_region(x, y, width, height, language);
    unsafe { RoUninitialize() };
    result
}

pub fn capture_png(x: i32, y: i32, width: i32, height: i32) -> Result<Vec<u8>, AppError> {
    if width < 8 || height < 8 {
        return Err(AppError::Ocr("所选区域太小".into()));
    }
    let mut pixels = capture_bgra(x, y, width, height, width, height)?;
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
        pixel[3] = 255;
    }
    let mut encoded = Vec::new();
    PngEncoder::new(&mut encoded)
        .write_image(
            &pixels,
            width as u32,
            height as u32,
            ExtendedColorType::Rgba8,
        )
        .map_err(|error| AppError::Ocr(format!("无法编码 OCR 截图：{error}")))?;
    Ok(encoded)
}

fn recognize_region(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    language: OcrLanguage,
) -> Result<String, AppError> {
    let max_dimension = OcrEngine::MaxImageDimension()
        .map_err(|error| AppError::Ocr(format!("无法读取 OCR 图像限制：{error}")))?
        .min(i32::MAX as u32) as i32;
    let (output_width, output_height) = ocr_dimensions(width, height, max_dimension);
    let pixels = capture_bgra(x, y, width, height, output_width, output_height)?;
    let engine = create_engine(language)?;
    let original = recognize_pixels(&engine, &pixels, output_width, output_height);
    let enhanced_pixels = enhance_for_ocr(&pixels);
    let enhanced = recognize_pixels(&engine, &enhanced_pixels, output_width, output_height);

    match (original, enhanced) {
        (Ok(original), Ok(enhanced)) => {
            if text_quality_score(&enhanced) > text_quality_score(&original) {
                Ok(enhanced)
            } else {
                Ok(original)
            }
        }
        (Ok(text), Err(_)) | (Err(_), Ok(text)) => Ok(text),
        (Err(error), Err(_)) => Err(error),
    }
}

fn create_engine(language: OcrLanguage) -> Result<OcrEngine, AppError> {
    match language {
        OcrLanguage::Auto => OcrEngine::TryCreateFromUserProfileLanguages(),
        OcrLanguage::Chinese => Language::CreateLanguage(&HSTRING::from("zh-Hans"))
            .and_then(|language| OcrEngine::TryCreateFromLanguage(&language)),
        OcrLanguage::English => Language::CreateLanguage(&HSTRING::from("en-US"))
            .and_then(|language| OcrEngine::TryCreateFromLanguage(&language)),
    }
    .map_err(|error| AppError::Ocr(format!("请安装所选 OCR 语言包：{error}")))
}

fn recognize_pixels(
    engine: &OcrEngine,
    pixels: &[u8],
    width: i32,
    height: i32,
) -> Result<String, AppError> {
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

    let bitmap =
        SoftwareBitmap::CreateCopyFromBuffer(&buffer, BitmapPixelFormat::Bgra8, width, height)
            .map_err(|error| AppError::Ocr(format!("无法创建 OCR 图像：{error}")))?;
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

fn ocr_dimensions(width: i32, height: i32, max_dimension: i32) -> (i32, i32) {
    let preferred_scale: f64 = if height <= 80 {
        3.0
    } else if height <= 180 {
        2.0
    } else {
        1.0
    };
    let scale = preferred_scale.min(max_dimension as f64 / width.max(height) as f64);
    (
        ((width as f64 * scale).round() as i32).max(1),
        ((height as f64 * scale).round() as i32).max(1),
    )
}

fn enhance_for_ocr(pixels: &[u8]) -> Vec<u8> {
    let luma = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| {
            (0.114 * pixel[0] as f32 + 0.587 * pixel[1] as f32 + 0.299 * pixel[2] as f32).round()
                as u8
        })
        .collect::<Vec<_>>();
    let (minimum, maximum) = luma
        .iter()
        .fold((u8::MAX, u8::MIN), |(minimum, maximum), value| {
            (minimum.min(*value), maximum.max(*value))
        });
    let span = maximum.saturating_sub(minimum).max(32) as f32;
    let mut enhanced = Vec::with_capacity(pixels.len());
    for value in luma {
        let normalized = (((value.saturating_sub(minimum)) as f32 / span) * 255.0)
            .round()
            .clamp(0.0, 255.0) as u8;
        enhanced.extend_from_slice(&[normalized, normalized, normalized, 255]);
    }
    enhanced
}

fn text_quality_score(text: &str) -> i64 {
    text.chars().fold(0, |score, character| {
        if character == '\u{fffd}' || character.is_control() && !character.is_whitespace() {
            score - 12
        } else if character.is_alphanumeric() || ('\u{3400}'..='\u{9fff}').contains(&character) {
            score + 5
        } else if character.is_whitespace() {
            score
        } else {
            score + 1
        }
    })
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
    use super::{ocr_dimensions, text_quality_score};

    #[test]
    fn upscales_text_strips_and_caps_large_regions() {
        assert_eq!(ocr_dimensions(500, 60, 2_600), (1_500, 180));
        assert_eq!(ocr_dimensions(800, 160, 2_600), (1_600, 320));
        assert_eq!(ocr_dimensions(800, 600, 2_600), (800, 600));
        assert_eq!(ocr_dimensions(3_840, 2_160, 2_600), (2_600, 1_463));
    }

    #[test]
    fn prefers_meaningful_text_over_replacement_characters() {
        assert!(text_quality_score("Hello 世界") > text_quality_score("He��o"));
    }
}
