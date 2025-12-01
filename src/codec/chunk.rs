use alloc::vec;
use alloc::vec::Vec;

use crate::{
    codec::{bits::BitUnpacker, lms::LMS_LEN},
    encoder::EncoderSettings,
};

use super::{
    bits::BitPacker,
    common::{SeaError, SeaResidualSize},
    file::SeaFileHeader,
    lms::SeaLMS,
};

#[derive(Debug, Clone, Copy)]
pub enum SeaChunkType {
    Cbr = 0x01,
    Vbr = 0x02,
}

#[derive(Debug)]
pub struct SeaChunk {
    pub channels: usize,
    pub frames_per_chunk: usize,

    pub chunk_type: SeaChunkType,

    pub scale_factor_bits: u8,
    pub scale_factor_frames: u8,
    pub residual_size: SeaResidualSize,

    pub lms: Vec<SeaLMS>,

    pub scale_factors: Vec<u8>,
    pub vbr_residual_sizes: Vec<u8>,
    pub residuals: Vec<u8>,
}

impl SeaChunk {
    pub fn new(
        file_header: &SeaFileHeader,
        lms: &[SeaLMS],
        encoder_settings: &EncoderSettings,
        scale_factors: Vec<u8>,
        vbr_residual_sizes: Vec<u8>,
        residuals: Vec<u8>,
    ) -> SeaChunk {
        let is_vbr = !vbr_residual_sizes.is_empty();
        let chunk_type = if is_vbr {
            SeaChunkType::Vbr
        } else {
            SeaChunkType::Cbr
        };

        SeaChunk {
            channels: file_header.channels as usize,
            frames_per_chunk: file_header.frames_per_chunk as usize,

            chunk_type,
            scale_factor_bits: encoder_settings.scale_factor_bits,
            scale_factor_frames: encoder_settings.scale_factor_frames,
            residual_size: SeaResidualSize::from(libm::floorf(encoder_settings.residual_bits) as u8),

            lms: Vec::from(lms),
            scale_factors,
            vbr_residual_sizes,
            residuals,
        }
    }

    pub fn from_slice(
        encoded: &[u8],
        file_header: &SeaFileHeader,
        remaining_frames: Option<usize>,
    ) -> Result<Self, SeaError> {
        assert!(encoded.len() <= file_header.chunk_size as usize);

        // we cannot calculate last frame size in streaming mode
        if remaining_frames.is_none() && encoded.len() < file_header.chunk_size as usize {
            return Err(SeaError::InvalidFrame);
        }

        let chunk_type: SeaChunkType = match encoded[0] {
            0x01 => SeaChunkType::Cbr,
            0x02 => SeaChunkType::Vbr,
            _ => return Err(SeaError::InvalidFrame),
        };

        let scale_factor_bits = encoded[1] >> 4;

        let residual_size = SeaResidualSize::from(encoded[1] & 0b1111);
        let scale_factor_frames = encoded[2];
        let _reserved = encoded[3];

        let mut encoded_index = 4;

        let mut lms: Vec<SeaLMS> = vec![];
        for _ in 0..file_header.channels as usize {
            lms.push(SeaLMS::from_bytes(
                &encoded[encoded_index..encoded_index + LMS_LEN * 4]
                    .try_into()
                    .unwrap(),
            ));
            encoded_index += LMS_LEN * 4;
        }

        let frames_in_this_chunk =
            (file_header.frames_per_chunk as usize).min(remaining_frames.unwrap_or(usize::MAX));

        let scale_factor_items = frames_in_this_chunk.div_ceil(scale_factor_frames as usize)
            * file_header.channels as usize;

        let scale_factors = {
            let packed_scale_factor_bytes =
                (scale_factor_items * scale_factor_bits as usize).div_ceil(8);

            let packed_scale_factors =
                &encoded[encoded_index..encoded_index + packed_scale_factor_bytes];
            encoded_index += packed_scale_factor_bytes;

            let mut unpacker = BitUnpacker::new_const_bits(scale_factor_bits);
            unpacker.process_bytes(packed_scale_factors);
            let mut res = unpacker.finish();
            res.resize(scale_factor_items, 0);
            res
        };

        let vbr_residual_sizes: Vec<u8> = if matches!(chunk_type, SeaChunkType::Vbr) {
            let packed_vbr_residual_sizes_bytes = (scale_factor_items * 2).div_ceil(8);
            let packed_vbr_residual_sizes =
                &encoded[encoded_index..encoded_index + packed_vbr_residual_sizes_bytes];
            encoded_index += packed_vbr_residual_sizes_bytes;

            let mut unpacker: BitUnpacker = BitUnpacker::new_const_bits(2);
            unpacker.process_bytes(packed_vbr_residual_sizes);
            let mut res = unpacker.finish();
            res.resize(scale_factor_items, 0);
            for item in &mut res {
                *item += residual_size as u8 - 1;
            }
            res
        } else {
            Vec::new()
        };

        let residuals: Vec<u8> = {
            let mut unpacker = if matches!(chunk_type, SeaChunkType::Vbr) {
                let mut bitlengths = Vec::new();
                for vbr_chunk in vbr_residual_sizes.chunks_exact(file_header.channels as usize) {
                    for _ in 0..scale_factor_frames {
                        for item in vbr_chunk.iter().take(file_header.channels as usize) {
                            bitlengths.push(*item);
                        }
                    }
                }

                BitUnpacker::new_var_bits(&bitlengths)
            } else {
                BitUnpacker::new_const_bits(residual_size as u8)
            };

            let packed_residuals_bytes = if matches!(chunk_type, SeaChunkType::Vbr) {
                let mut residual_bits: u32 = vbr_residual_sizes
                    [..vbr_residual_sizes.len() - file_header.channels as usize]
                    .iter()
                    .map(|x| *x as u32)
                    .sum();

                residual_bits *= scale_factor_frames as u32;

                let last_frame_samples = frames_in_this_chunk as u32 % scale_factor_frames as u32;
                let multiplier = if last_frame_samples == 0 {
                    scale_factor_frames as u32
                } else {
                    last_frame_samples
                };

                for size in vbr_residual_sizes
                    [(vbr_residual_sizes.len() - file_header.channels as usize)..]
                    .iter()
                {
                    residual_bits += *size as u32 * multiplier;
                }

                let residual_bytes = residual_bits.div_ceil(8);
                residual_bytes as usize
            } else {
                (frames_in_this_chunk * residual_size as usize * file_header.channels as usize)
                    .div_ceil(8)
            };

            let packed_residuals = &encoded[encoded_index..encoded_index + packed_residuals_bytes];

            unpacker.process_bytes(packed_residuals);

            let mut res = unpacker.finish();
            res.resize(frames_in_this_chunk * file_header.channels as usize, 0);
            res
        };

        Ok(Self {
            channels: file_header.channels as usize,
            frames_per_chunk: file_header.frames_per_chunk as usize,

            chunk_type,
            scale_factor_bits,
            scale_factor_frames,
            residual_size,

            lms,
            scale_factors,
            vbr_residual_sizes,
            residuals,
        })
    }

    fn serialize_header(&self) -> [u8; 4] {
        assert!(self.scale_factor_bits > 0);
        assert!(self.scale_factor_frames > 0);
        assert_eq!(self.frames_per_chunk % self.scale_factor_frames as usize, 0);

        [
            self.chunk_type as u8,
            (self.scale_factor_bits << 4) | self.residual_size as u8,
            self.scale_factor_frames,
            0x5A,
        ]
    }

    fn serialize_lms(&self) -> Vec<u8> {
        assert_eq!(self.channels, self.lms.len());

        self.lms
            .iter()
            .flat_map(|lms| lms.serialize())
            .collect::<Vec<_>>()
    }

    fn serialize_scale_factors(&self) -> Vec<u8> {
        let mut packer = BitPacker::new();
        for scale_factor in self.scale_factors.iter() {
            packer.push(*scale_factor as u32, self.scale_factor_bits);
        }
        packer.finish()
    }

    fn serialize_vbr_residual_sizes(&self) -> Vec<u8> {
        let mut packer = BitPacker::new();
        for vbr_residual_size in self.vbr_residual_sizes.iter() {
            let relative_size = *vbr_residual_size as i32 - self.residual_size as i32 + 1;
            packer.push(relative_size as u32, 2);
        }
        packer.finish()
    }

    fn serialize_residuals(&self) -> Vec<u8> {
        let mut packer = BitPacker::new();
        if matches!(self.chunk_type, SeaChunkType::Vbr) {
            let mut vbr_residual_index = 0;
            let mut frames_written_since_update = 0;
            for residual in self.residuals.chunks_exact(self.channels) {
                for (channel_index, item) in residual.iter().enumerate().take(self.channels) {
                    packer.push(
                        *item as u32,
                        self.vbr_residual_sizes[vbr_residual_index + channel_index],
                    );
                }
                frames_written_since_update += 1;
                if frames_written_since_update == self.scale_factor_frames {
                    vbr_residual_index += self.channels;
                    frames_written_since_update = 0;
                }
            }
        } else {
            for residual in self.residuals.iter() {
                packer.push(*residual as u32, self.residual_size as u8);
            }
        }
        packer.finish()
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut output = Vec::new();

        output.extend_from_slice(&self.serialize_header());
        output.extend_from_slice(&self.serialize_lms());
        output.extend_from_slice(&self.serialize_scale_factors());
        if matches!(self.chunk_type, SeaChunkType::Vbr) {
            output.extend_from_slice(&self.serialize_vbr_residual_sizes());
        }
        output.extend_from_slice(&self.serialize_residuals());

        output
    }
}
