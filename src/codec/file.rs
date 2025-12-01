use alloc::{rc::Rc, string::String, vec::Vec};

use crate::{
    codec::{chunk::SeaChunk, common::read_max_or_zero},
    cursor::Cursor,
    encoder::EncoderSettings,
};

use super::{
    chunk::SeaChunkType,
    common::{
        read_u16_le, read_u32_be, read_u32_le, read_u8, SeaEncoderTrait, SeaError, SEAC_MAGIC,
    },
    decoder::Decoder,
    encoder_cbr::CbrEncoder,
    encoder_vbr::VbrEncoder,
};

#[derive(Debug, Clone)]
pub struct SeaFileHeader {
    pub version: u8,
    pub channels: u8,
    pub chunk_size: u16,
    pub frames_per_chunk: u16,
    pub sample_rate: u32,
    pub total_frames: u32,
    pub metadata: Rc<String>,
}

impl SeaFileHeader {
    fn validate(&self) -> bool {
        self.channels > 0
            && self.chunk_size >= 16
            && self.frames_per_chunk > 0
            && self.sample_rate > 0
    }

    pub fn from_reader(reader: &mut Cursor) -> Result<Self, SeaError> {
        let magic = read_u32_be(reader)?;
        if magic != SEAC_MAGIC {
            return Err(SeaError::InvalidFile);
        }
        let version = read_u8(reader)?;
        let channels = read_u8(reader)?;
        let chunk_size = read_u16_le(reader)?;
        let frames_per_chunk = read_u16_le(reader)?;
        let sample_rate = read_u32_le(reader)?;
        let total_frames = read_u32_le(reader)?;
        let metadata_size = read_u32_le(reader)?;

        let mut metadata = Vec::<u8>::with_capacity(metadata_size as usize);
        reader.read_exact(&mut metadata)?;
        let metadata_string = String::from_utf8(metadata).unwrap();

        let res: SeaFileHeader = Self {
            version,
            channels,
            chunk_size,
            frames_per_chunk,
            sample_rate,
            total_frames,
            metadata: Rc::new(metadata_string),
        };

        if !res.validate() {
            return Err(SeaError::InvalidFile);
        }

        Ok(res)
    }

    pub fn set_total_frames(&mut self, total_frames: u32) {
        self.total_frames = total_frames;
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut output = Vec::new();

        output.extend_from_slice(&SEAC_MAGIC.to_be_bytes());
        output.extend_from_slice(&self.version.to_le_bytes());
        output.extend_from_slice(&self.channels.to_le_bytes());
        output.extend_from_slice(&self.chunk_size.to_le_bytes());
        output.extend_from_slice(&self.frames_per_chunk.to_le_bytes());
        output.extend_from_slice(&self.sample_rate.to_le_bytes());
        output.extend_from_slice(&self.total_frames.to_le_bytes());
        let metadata_len_u32 = self.metadata.len() as u32;
        output.extend_from_slice(&metadata_len_u32.to_le_bytes());
        output.extend_from_slice(self.metadata.as_bytes());

        output
    }
}

enum ActiveEncoder {
    Cbr(CbrEncoder),
    Vbr(VbrEncoder),
}

pub struct SeaFile {
    pub header: SeaFileHeader,

    decoder: Option<Decoder>,

    encoder: Option<ActiveEncoder>,
    encoder_settings: Option<EncoderSettings>,
}

impl SeaFile {
    pub fn new(
        header: SeaFileHeader,
        encoder_settings: &EncoderSettings,
    ) -> Result<Self, SeaError> {
        let encoder = if encoder_settings.vbr {
            let vbr_encoder = VbrEncoder::new(&header, &encoder_settings.clone());
            Some(ActiveEncoder::Vbr(vbr_encoder))
        } else {
            let cbr_encoder = CbrEncoder::new(&header, &encoder_settings.clone());
            Some(ActiveEncoder::Cbr(cbr_encoder))
        };

        Ok(SeaFile {
            header,
            decoder: None,
            encoder,
            encoder_settings: Some(encoder_settings.clone()),
        })
    }

    pub fn from_reader(reader: &mut Cursor) -> Result<Self, SeaError> {
        let header = SeaFileHeader::from_reader(reader)?;

        Ok(SeaFile {
            header,
            decoder: None,
            encoder: None,
            encoder_settings: None,
        })
    }

    pub fn make_chunk(&mut self, samples: &[i16]) -> Result<Vec<u8>, SeaError> {
        let encoder_settings = self.encoder_settings.as_ref().unwrap();
        let encoder = self.encoder.as_mut().unwrap();

        let initial_lms = match encoder {
            ActiveEncoder::Cbr(encoder) => encoder.get_lms().clone(),
            ActiveEncoder::Vbr(encoder) => encoder.get_lms().clone(),
        };

        let encoded = match encoder {
            ActiveEncoder::Cbr(encoder) => encoder.encode(samples),
            ActiveEncoder::Vbr(encoder) => encoder.encode(samples),
        };

        let chunk = SeaChunk::new(
            &self.header,
            &initial_lms,
            encoder_settings,
            encoded.scale_factors,
            encoded.residual_bits,
            encoded.residuals,
        );
        let output = chunk.serialize();

        if self.header.chunk_size == 0 {
            self.header.chunk_size = output.len() as u16;
        }

        let full_samples_len =
            self.header.frames_per_chunk as usize * self.header.channels as usize;

        if samples.len() == full_samples_len {
            assert_eq!(self.header.chunk_size, output.len() as u16);
        }

        Ok(output)
    }

    pub fn samples_from_reader(
        &mut self,
        reader: &mut Cursor,
        remaining_frames: Option<usize>,
    ) -> Result<Option<Vec<i16>>, SeaError> {
        let encoded = read_max_or_zero(reader, self.header.chunk_size as usize)?;
        if encoded.is_empty() {
            return Ok(None);
        }

        let chunk = SeaChunk::from_slice(&encoded, &self.header, remaining_frames);

        match chunk {
            Ok(chunk) => {
                if self.decoder.is_none() {
                    self.decoder = Some(Decoder::init(
                        self.header.channels as usize,
                        chunk.scale_factor_bits as usize,
                    ));
                }
                let decoder = self.decoder.as_mut().unwrap();
                let decoded = match chunk.chunk_type {
                    SeaChunkType::Cbr => decoder.decode_cbr(&chunk),
                    SeaChunkType::Vbr => decoder.decode_vbr(&chunk),
                };
                Ok(Some(decoded))
            }
            Err(err) => Err(err),
        }
    }
}
