#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use decoder::SeaDecoder;
use encoder::{EncoderSettings, SeaEncoder};

mod codec;
mod cursor;
pub mod decoder;
pub mod encoder;
#[cfg(all(target_arch = "wasm32", feature = "wasm-api"))]
pub mod wasm_api;

#[cfg(feature = "c-api")]
pub mod c_api;

pub fn sea_encode(
    input_samples: &[i16],
    sample_rate: u32,
    channels: u32,
    settings: EncoderSettings,
) -> Vec<u8> {
    let mut sea_encoded = Vec::<u8>::with_capacity(input_samples.len());
    let mut sea_encoder = SeaEncoder::from_slice(
        channels as u8,
        sample_rate,
        Some(input_samples.len() as u32 / channels),
        settings,
        input_samples,
    )
    .unwrap();

    while sea_encoder.encode_frame(&mut sea_encoded).unwrap() {}
    sea_encoder.finalize().unwrap();

    sea_encoded
}

pub struct SeaDecodeInfo {
    pub samples: Vec<i16>,
    pub sample_rate: u32,
    pub channels: u32,
}

pub fn sea_decode(encoded: &[u8]) -> SeaDecodeInfo {
    let mut sea_decoded = Vec::<i16>::with_capacity(encoded.len() * 8);

    let mut sea_decoder = SeaDecoder::from_slice(encoded).unwrap();

    while sea_decoder.decode_frame(&mut sea_decoded).unwrap() {}

    let header = sea_decoder.get_header();

    SeaDecodeInfo {
        samples: sea_decoded,
        sample_rate: header.sample_rate,
        channels: header.channels as u32,
    }
}
