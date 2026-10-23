use rcode::{DecodeResult, Error, decode_luma8};
use std::panic::{AssertUnwindSafe, catch_unwind};

const OK: u32 = 0;
const INVALID_ARGUMENT: u32 = 1;
const NO_CODE: u32 = 2;
const CORRUPT_CODE: u32 = 3;
const INVALID_IMAGE: u32 = 4;
const INTERNAL_PANIC: u32 = 5;

/// Owns a decoded result. Its payload pointer is borrowed until this is freed.
pub struct RCodeResult {
    decoded: DecodeResult,
}

#[unsafe(no_mangle)]
pub extern "C" fn rcode_abi_version() -> u32 {
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn rcode_core_version() -> *const std::ffi::c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr().cast()
}

#[unsafe(no_mangle)]
pub extern "C" fn rcode_status_message(status: u32) -> *const std::ffi::c_char {
    let message = match status {
        OK => c"ok",
        INVALID_ARGUMENT => c"invalid argument",
        NO_CODE => c"no code found",
        CORRUPT_CODE => c"corrupt code",
        INVALID_IMAGE => c"invalid image",
        INTERNAL_PANIC => c"internal panic",
        _ => c"unknown status",
    };
    message.as_ptr()
}

#[unsafe(no_mangle)]
/// # Safety
/// `pixels` must point to `pixels_len` readable bytes and `out_result` to writable storage.
pub unsafe extern "C" fn rcode_decode_luma8(
    pixels: *const u8,
    pixels_len: usize,
    width: u32,
    height: u32,
    row_stride: usize,
    out_result: *mut *mut RCodeResult,
) -> u32 {
    if out_result.is_null() {
        return INVALID_ARGUMENT;
    }
    // SAFETY: out_result was checked for null; the caller guarantees writable storage.
    unsafe { *out_result = std::ptr::null_mut() };
    if pixels.is_null() || pixels_len > isize::MAX as usize {
        return INVALID_ARGUMENT;
    }
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: a non-null pointer and its readable extent are required by this C API.
        let pixels = unsafe { std::slice::from_raw_parts(pixels, pixels_len) };
        match decode_luma8(pixels, width, height, row_stride) {
            Ok(decoded) => {
                // SAFETY: out_result was checked and is writable per the caller contract.
                unsafe { *out_result = Box::into_raw(Box::new(RCodeResult { decoded })) };
                OK
            }
            Err(error) => status_for_error(&error),
        }
    }))
    .unwrap_or(INTERNAL_PANIC)
}

#[unsafe(no_mangle)]
/// # Safety
/// `result` must be a live result handle and both output pointers must be writable.
pub unsafe extern "C" fn rcode_result_payload(
    result: *const RCodeResult,
    out_payload: *mut *const u8,
    out_length: *mut usize,
) -> u32 {
    if result.is_null() || out_payload.is_null() || out_length.is_null() {
        return INVALID_ARGUMENT;
    }
    // SAFETY: all pointers are non-null and writable storage is part of the C contract.
    let result = unsafe { &*result };
    unsafe {
        *out_payload = result.decoded.payload.as_ptr();
        *out_length = result.decoded.payload.len();
    }
    OK
}

#[unsafe(no_mangle)]
/// # Safety
/// `result` must be a live result handle and `out_count` must be writable.
pub unsafe extern "C" fn rcode_result_corrected_bytes(
    result: *const RCodeResult,
    out_count: *mut usize,
) -> u32 {
    if result.is_null() || out_count.is_null() {
        return INVALID_ARGUMENT;
    }
    // SAFETY: pointers were checked; out_count is writable per the C contract.
    unsafe {
        *out_count = (*result).decoded.corrected_bytes;
    }
    OK
}

#[unsafe(no_mangle)]
/// # Safety
/// `result` must be a live result handle and both output pointers must be writable.
pub unsafe extern "C" fn rcode_result_landmark(
    result: *const RCodeResult,
    index: u32,
    out_x: *mut f64,
    out_y: *mut f64,
) -> u32 {
    if result.is_null() || out_x.is_null() || out_y.is_null() {
        return INVALID_ARGUMENT;
    }
    // SAFETY: result is non-null; bounds are checked before access, outputs are writable.
    // SAFETY: result is a live handle per the caller contract.
    let landmarks = unsafe { &(*result).decoded.landmarks };
    let Some(point) = landmarks.get(index as usize) else {
        return INVALID_ARGUMENT;
    };
    unsafe {
        *out_x = point.x;
        *out_y = point.y;
    }
    OK
}

#[unsafe(no_mangle)]
/// # Safety
/// `result` must be a live result handle and both output pointers must be writable.
pub unsafe extern "C" fn rcode_result_image_size(
    result: *const RCodeResult,
    out_width: *mut u32,
    out_height: *mut u32,
) -> u32 {
    if result.is_null() || out_width.is_null() || out_height.is_null() {
        return INVALID_ARGUMENT;
    }
    // SAFETY: pointers were checked; outputs are writable per the C contract.
    unsafe {
        *out_width = (*result).decoded.width;
        *out_height = (*result).decoded.height;
    }
    OK
}

#[unsafe(no_mangle)]
/// # Safety
/// `result` must be a live result handle and both output pointers must be writable.
pub unsafe extern "C" fn rcode_result_profile(
    result: *const RCodeResult,
    out_spokes: *mut u32,
    out_ecc_id: *mut u32,
) -> u32 {
    if result.is_null() || out_spokes.is_null() || out_ecc_id.is_null() {
        return INVALID_ARGUMENT;
    }
    // SAFETY: pointers were checked; outputs are writable per the C contract.
    let profile = unsafe { (*result).decoded.profile };
    let ecc_id = match profile.level {
        rcode::EccLevel::L => 0,
        rcode::EccLevel::M => 1,
        rcode::EccLevel::Q => 2,
        rcode::EccLevel::H => 3,
    };
    unsafe {
        *out_spokes = profile.spokes() as u32;
        *out_ecc_id = ecc_id;
    }
    OK
}

#[unsafe(no_mangle)]
/// # Safety
/// `result` must be null or a live handle returned by `rcode_decode_luma8` that has not been freed.
pub unsafe extern "C" fn rcode_result_free(result: *mut RCodeResult) {
    if !result.is_null() {
        // SAFETY: non-null values are pointers returned by Box::into_raw above.
        drop(unsafe { Box::from_raw(result) });
    }
}

fn status_for_error(error: &Error) -> u32 {
    match error {
        Error::InvalidOptions(_) => INVALID_ARGUMENT,
        Error::NoCode => NO_CODE,
        Error::CorruptCode(_) => CORRUPT_CODE,
        Error::InvalidImage(_) => INVALID_IMAGE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageReader;
    use rcode::{EccLevel, Profile, RenderOptions, Style, encode, render_png};
    use std::io::Cursor;

    fn luma_frame(style: Style) -> (Vec<u8>, u32, u32) {
        let profile = Profile::new(54, EccLevel::Q).unwrap();
        let symbol = encode(b"ffi camera frame", profile).unwrap();
        let png = render_png(
            &symbol,
            &RenderOptions {
                size: 512,
                style,
                ..RenderOptions::default()
            },
        )
        .unwrap();
        let gray = ImageReader::new(Cursor::new(png))
            .with_guessed_format()
            .unwrap()
            .decode()
            .unwrap()
            .to_luma8();
        (gray.into_raw(), 512, 512)
    }

    fn decode(frame: &[u8], width: u32, height: u32, stride: usize) -> *mut RCodeResult {
        let mut result = std::ptr::null_mut();
        // SAFETY: frame is a readable byte slice and result is writable local storage.
        let status = unsafe {
            rcode_decode_luma8(
                frame.as_ptr(),
                frame.len(),
                width,
                height,
                stride,
                &mut result,
            )
        };
        assert_eq!(status, OK);
        result
    }

    #[test]
    fn exported_decode_handles_packed_and_padded_luma() {
        let (packed, width, height) = luma_frame(Style::Lines);
        let result = decode(&packed, width, height, width as usize);
        let (mut data, mut len) = (std::ptr::null(), 0);
        let (mut spokes, mut ecc) = (0, 0);
        let (mut image_width, mut image_height) = (0, 0);
        // SAFETY: result is valid; output references point to local writable values.
        unsafe {
            assert_eq!(rcode_result_payload(result, &mut data, &mut len), OK);
            assert_eq!(std::slice::from_raw_parts(data, len), b"ffi camera frame");
            let mut corrected = 0;
            assert_eq!(rcode_result_corrected_bytes(result, &mut corrected), OK);
            assert_eq!(rcode_result_profile(result, &mut spokes, &mut ecc), OK);
            assert_eq!((spokes, ecc), (54, 2));
            assert_eq!(
                rcode_result_image_size(result, &mut image_width, &mut image_height),
                OK
            );
            assert_eq!((image_width, image_height), (width, height));
            let mut landmarks = Vec::new();
            for index in 0..4 {
                let (mut x, mut y) = (0., 0.);
                assert_eq!(rcode_result_landmark(result, index, &mut x, &mut y), OK);
                landmarks.push((x, y));
            }
            assert_eq!(landmarks.len(), 4);
            assert!(landmarks[0].0 < landmarks[1].0 && landmarks[0].1 < landmarks[2].1);
            let (mut x, mut y) = (0., 0.);
            assert_eq!(
                rcode_result_landmark(result, 4, &mut x, &mut y),
                INVALID_ARGUMENT
            );
            rcode_result_free(result);
            rcode_result_free(std::ptr::null_mut());
        }

        let (packed, width, height) = luma_frame(Style::Dots);
        let stride = width as usize + 7;
        let mut padded = vec![255; stride * height as usize];
        for row in 0..height as usize {
            let source = &packed[row * width as usize..(row + 1) * width as usize];
            padded[row * stride..row * stride + width as usize].copy_from_slice(source);
        }
        let result = decode(&padded, width, height, stride);
        // SAFETY: result is valid and owned here.
        unsafe { rcode_result_free(result) };
    }

    #[test]
    fn exported_decode_rejects_null_inputs_and_outputs() {
        let mut result = std::ptr::dangling_mut::<RCodeResult>();
        // SAFETY: out result is writable; the pixel pointer is intentionally null.
        assert_eq!(
            unsafe { rcode_decode_luma8(std::ptr::null(), 4, 2, 2, 2, &mut result) },
            INVALID_ARGUMENT
        );
        assert!(result.is_null());
        // SAFETY: intentionally null output pointer is checked before access.
        assert_eq!(
            unsafe { rcode_decode_luma8([0u8; 4].as_ptr(), 4, 2, 2, 2, std::ptr::null_mut()) },
            INVALID_ARGUMENT
        );
        let byte = 0u8;
        // SAFETY: oversized lengths are rejected before constructing a slice.
        assert_eq!(
            unsafe { rcode_decode_luma8(&byte, isize::MAX as usize + 1, 2, 2, 2, &mut result) },
            INVALID_ARGUMENT
        );
    }
}
