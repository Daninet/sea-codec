use alloc::vec::Vec;

use super::{chunk::SeaChunk, common::clamp_i16, dqt::SeaDequantTab};

pub struct Decoder {
    channels: usize,
    scale_factor_bits: usize,

    dequant_tab: SeaDequantTab,
}

impl Decoder {
    pub fn init(channels: usize, scale_factor_bits: usize) -> Self {
        Self {
            channels,
            scale_factor_bits,

            dequant_tab: SeaDequantTab::init(scale_factor_bits),
        }
    }

    pub fn decode_cbr(&self, chunk: &SeaChunk) -> Vec<i16> {
        assert_eq!(chunk.scale_factor_bits as usize, self.scale_factor_bits);

        let mut output: Vec<i16> = Vec::with_capacity(chunk.frames_per_chunk * self.channels);

        let mut lms = chunk.lms.clone();

        let dqts: &Vec<Vec<i32>> = self.dequant_tab.get_dqt(chunk.residual_size as usize);

        for (scale_factor_index, subchunk_residuals) in chunk
            .residuals
            .chunks(self.channels * chunk.scale_factor_frames as usize)
            .enumerate()
        {
            let scale_factors = &chunk.scale_factors[scale_factor_index * self.channels..];

            for channel_residuals in subchunk_residuals.chunks(self.channels) {
                for (channel_index, residual) in channel_residuals.iter().enumerate() {
                    let scale_factor = scale_factors[channel_index];
                    let predicted = lms[channel_index].predict();
                    let quantized: usize = *residual as usize;
                    let dequantized = dqts[scale_factor as usize][quantized];
                    let reconstructed = clamp_i16(predicted + dequantized);
                    output.push(reconstructed);
                    lms[channel_index].update(reconstructed, dequantized);
                }
            }
        }

        output
    }

    pub fn decode_vbr(&self, chunk: &SeaChunk) -> Vec<i16> {
        assert_eq!(chunk.scale_factor_bits as usize, self.scale_factor_bits);

        let mut output: Vec<i16> = Vec::with_capacity(chunk.frames_per_chunk * self.channels);

        let mut lms = chunk.lms.clone();

        let dqts: &Vec<Vec<Vec<i32>>> = &(1..=8)
            .map(|i| self.dequant_tab.get_dqt(i).clone())
            .collect();

        for (scale_factor_index, subchunk_residuals) in chunk
            .residuals
            .chunks(self.channels * chunk.scale_factor_frames as usize)
            .enumerate()
        {
            let scale_factors = &chunk.scale_factors[scale_factor_index * self.channels..];
            let vbr_residuals = &chunk.vbr_residual_sizes[scale_factor_index * self.channels..];

            for channel_residuals in subchunk_residuals.chunks(self.channels) {
                for (channel_index, residual) in channel_residuals.iter().enumerate() {
                    let residual_size: usize = vbr_residuals[channel_index] as usize;
                    let scale_factor = scale_factors[channel_index];
                    let predicted = lms[channel_index].predict();
                    let quantized: usize = *residual as usize;
                    let dequantized = dqts[residual_size - 1][scale_factor as usize][quantized];
                    let reconstructed = clamp_i16(predicted + dequantized);
                    output.push(reconstructed);
                    lms[channel_index].update(reconstructed, dequantized);
                }
            }
        }

        output
    }
}
