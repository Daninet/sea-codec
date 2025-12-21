use crate::{
    encoder::EncoderSettings, sea_decode as rust_sea_decode, sea_encode as rust_sea_encode,
};
use alloc::vec::Vec;
use core::ffi::c_float;
use core::slice;

#[repr(C)]
pub struct CSeaEncoderSettings {
    pub scale_factor_bits: u8,
    pub scale_factor_frames: u8,
    pub residual_bits: c_float,
    pub frames_per_chunk: u16,
    pub vbr: bool,
}

impl From<&CSeaEncoderSettings> for EncoderSettings {
    fn from(c_settings: &CSeaEncoderSettings) -> Self {
        Self {
            scale_factor_bits: c_settings.scale_factor_bits,
            scale_factor_frames: c_settings.scale_factor_frames,
            residual_bits: c_settings.residual_bits,
            frames_per_chunk: c_settings.frames_per_chunk,
            vbr: c_settings.vbr,
        }
    }
}

#[no_mangle]
pub extern "C" fn sea_encoder_default_settings() -> CSeaEncoderSettings {
    let default = EncoderSettings::default();
    CSeaEncoderSettings {
        scale_factor_bits: default.scale_factor_bits,
        scale_factor_frames: default.scale_factor_frames,
        residual_bits: default.residual_bits,
        frames_per_chunk: default.frames_per_chunk,
        vbr: default.vbr,
    }
}

#[no_mangle]
pub unsafe extern "C" fn sea_encode(
    input_samples: *const i16,
    input_length: usize,
    sample_rate: u32,
    channels: u32,
    settings: *const CSeaEncoderSettings,
    output_data: *mut *mut u8,
    output_length: *mut usize,
) -> i32 {
    if input_samples.is_null() || output_data.is_null() || output_length.is_null() {
        return -1;
    }

    let input_slice = slice::from_raw_parts(input_samples, input_length);
    let encoder_settings = if settings.is_null() {
        EncoderSettings::default()
    } else {
        EncoderSettings::from(&*settings)
    };

    let mut encoded = rust_sea_encode(input_slice, sample_rate, channels, encoder_settings);

    encoded.shrink_to_fit();
    let ptr = encoded.as_mut_ptr();
    let len = encoded.len();
    core::mem::forget(encoded);

    *output_data = ptr;
    *output_length = len;

    0
}

#[no_mangle]
pub unsafe extern "C" fn sea_decode(
    encoded_data: *const u8,
    encoded_length: usize,
    output_samples: *mut *mut i16,
    output_sample_count: *mut usize,
    output_sample_rate: *mut u32,
    output_channels: *mut u32,
) -> i32 {
    if encoded_data.is_null() || output_samples.is_null() || output_sample_count.is_null() {
        return -1;
    }

    let encoded_slice = slice::from_raw_parts(encoded_data, encoded_length);

    let decode_info = rust_sea_decode(encoded_slice);

    let mut samples = decode_info.samples;
    samples.shrink_to_fit();
    let ptr = samples.as_mut_ptr();
    let len = samples.len();
    core::mem::forget(samples);

    *output_samples = ptr;
    *output_sample_count = len;

    if !output_sample_rate.is_null() {
        *output_sample_rate = decode_info.sample_rate;
    }
    if !output_channels.is_null() {
        *output_channels = decode_info.channels;
    }

    0
}

#[no_mangle]
pub unsafe extern "C" fn sea_free_packet(data: *mut u8, length: usize) {
    if !data.is_null() {
        let _ = Vec::from_raw_parts(data, length, length);
    }
}

#[no_mangle]
pub unsafe extern "C" fn sea_free_samples(samples: *mut i16, length: usize) {
    if !samples.is_null() {
        let _ = Vec::from_raw_parts(samples, length, length);
    }
}
