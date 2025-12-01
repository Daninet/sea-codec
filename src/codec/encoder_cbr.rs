use alloc::vec;
use alloc::vec::Vec;

use crate::encoder::EncoderSettings;

use super::{
    common::{EncodedSamples, SeaEncoderTrait, SeaResidualSize},
    encoder_base::EncoderBase,
    file::SeaFileHeader,
    lms::SeaLMS,
};

pub struct CbrEncoder {
    channels: usize,
    residual_size: SeaResidualSize,
    scale_factor_frames: usize,
    base_encoder: EncoderBase,
}

impl CbrEncoder {
    pub fn new(file_header: &SeaFileHeader, encoder_settings: &EncoderSettings) -> Self {
        CbrEncoder {
            channels: file_header.channels as usize,
            residual_size: SeaResidualSize::from(libm::floorf(encoder_settings.residual_bits) as u8),
            scale_factor_frames: encoder_settings.scale_factor_frames as usize,
            base_encoder: EncoderBase::new(
                file_header.channels as usize,
                encoder_settings.scale_factor_bits as usize,
            ),
        }
    }

    pub fn get_lms(&self) -> &Vec<SeaLMS> {
        &self.base_encoder.lms
    }
}

impl SeaEncoderTrait for CbrEncoder {
    fn encode(&mut self, samples: &[i16]) -> EncodedSamples {
        let mut scale_factors =
            vec![
                0u8;
                (samples.len() / self.channels).div_ceil(self.scale_factor_frames) * self.channels
            ];

        let mut residuals: Vec<u8> = vec![0u8; samples.len()];

        let mut ranks = vec![0u64; self.channels];

        let slice_size = self.scale_factor_frames * self.channels;

        let residual_sizes = vec![self.residual_size; self.channels];

        for (slice_index, input_slice) in samples.chunks(slice_size).enumerate() {
            self.base_encoder.get_residuals_for_chunk(
                input_slice,
                &residual_sizes,
                &mut scale_factors[slice_index * self.channels..],
                &mut residuals[slice_index * slice_size..],
                &mut ranks,
            );
        }

        EncodedSamples {
            scale_factors,
            residuals,
            residual_bits: vec![],
        }
    }
}
