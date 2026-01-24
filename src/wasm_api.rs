extern "C" {
    fn js_error(ptr: *const std::os::raw::c_char);
}

#[no_mangle]
pub extern "C" fn setup() {
    use std::ffi::CString;
    use std::panic;

    panic::set_hook(Box::new(|info| {
        let file = info.location().unwrap().file();
        let line = info.location().unwrap().line();
        let col = info.location().unwrap().column();

        let msg = match info.payload().downcast_ref::<&'static str>() {
            Some(s) => *s,
            None => match info.payload().downcast_ref::<String>() {
                Some(s) => &s[..],
                None => "Box<Any>",
            },
        };

        let err_info = format!("Panicked at '{}', {}:{}:{}", msg, file, line, col);
        let cstring = CString::new(err_info).unwrap();

        unsafe {
            js_error(cstring.as_ptr());
        }
    }));
}

#[no_mangle]
pub extern "C" fn wasm_sea_encode(
    input_samples: *const i16,
    input_length: usize,
    sample_rate: u32,
    channels: u32,
    bitrate: f32,
    vbr: bool,
    output_buffer: *mut u8,
    output_length: usize,
) -> usize {
    use crate::encoder::EncoderSettings;
    use crate::sea_encode;

    let input_samples = unsafe { std::slice::from_raw_parts(input_samples, input_length / 2) };
    let encoded_data = sea_encode(
        input_samples,
        sample_rate,
        channels,
        EncoderSettings {
            residual_bits: bitrate,
            vbr,
            ..Default::default()
        },
    );

    assert!(encoded_data.len() <= output_length);

    unsafe {
        std::ptr::copy_nonoverlapping(encoded_data.as_ptr(), output_buffer, encoded_data.len());
    }

    encoded_data.len()
}

#[no_mangle]
pub extern "C" fn wasm_sea_decode(
    encoded: *const u8,
    encoded_length: usize,
    output_buffer: *mut i16,
    output_length: usize,
    sample_rate: *mut u32,
    channels: *mut u32,
) -> usize {
    use crate::sea_decode;

    let encoded_data = unsafe { std::slice::from_raw_parts(encoded, encoded_length) };
    let decoded_data = sea_decode(encoded_data);

    assert!(decoded_data.samples.len() * 2 <= output_length);

    unsafe {
        std::ptr::copy_nonoverlapping(
            decoded_data.samples.as_ptr(),
            output_buffer,
            decoded_data.samples.len(),
        );

        *sample_rate = decoded_data.sample_rate;
        *channels = decoded_data.channels;
    }

    decoded_data.samples.len() * 2
}

#[no_mangle]
pub unsafe extern "C" fn allocate(size: usize) -> *mut u8 {
    use std::alloc::{alloc, Layout};

    let layout = Layout::from_size_align(size, 1).unwrap();
    alloc(layout)
}

#[no_mangle]
pub unsafe extern "C" fn deallocate(ptr: *mut u8, size: usize) {
    use std::alloc::{dealloc, Layout};

    let layout = Layout::from_size_align(size, 1).unwrap();
    dealloc(ptr, layout);
}

#[no_mangle]
pub extern "C" fn wasm_resample(
    input_samples: *const i16,
    input_length: usize,
    source_rate: u32,
    target_rate: u32,
    channels: u32,
    output_buffer: *mut i16,
    output_length: usize,
) -> usize {
    use crate::resample::resample;

    let input_samples = unsafe { std::slice::from_raw_parts(input_samples, input_length / 2) };
    let resampled = resample(input_samples, source_rate, target_rate, channels);

    assert!(resampled.len() * 2 <= output_length);

    unsafe {
        std::ptr::copy_nonoverlapping(resampled.as_ptr(), output_buffer, resampled.len());
    }

    resampled.len() * 2
}
