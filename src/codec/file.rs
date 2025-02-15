use std::{
    io::{self},
    rc::Rc,
};

use crate::codec::{chunk::SeaChunk, common::read_max_or_zero};

use super::{
    common::{
        read_u16_le, read_u32_be, read_u32_le, read_u8, SeaEncoderTrait, SeaError, SEAC_MAGIC,
    },
    dqt::SeaDequantTab,
    encoder::EncoderSettings,
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

    pub fn from_reader<R: io::Read>(mut reader: &mut R) -> Result<Self, SeaError> {
        let magic = read_u32_be(&mut reader)?;
        if magic != SEAC_MAGIC {
            return Err(SeaError::InvalidFile);
        }
        let version = read_u8(&mut reader)?;
        let channels = read_u8(&mut reader)?;
        let chunk_size = read_u16_le(&mut reader)?;
        let frames_per_chunk = read_u16_le(&mut reader)?;
        let sample_rate = read_u32_le(&mut reader)?;
        let total_frames = read_u32_le(&mut reader)?;
        let metadata_size = read_u32_le(&mut reader)?;

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
        output.extend_from_slice(&self.metadata.as_bytes());

        output
    }
}

pub struct SeaFile {
    pub header: SeaFileHeader,
    pub dequant_tab: SeaDequantTab,

    cbr_encoder: Option<CbrEncoder>,
    vbr_encoder: Option<VbrEncoder>,
    encoder_settings: Option<EncoderSettings>,
}

impl SeaFile {
    pub fn new(
        header: SeaFileHeader,
        encoder_settings: &EncoderSettings,
    ) -> Result<Self, SeaError> {
        let cbr_encoder = CbrEncoder::new(&header, &encoder_settings.clone());
        let vbr_encoder = VbrEncoder::new(&header, &encoder_settings.clone());

        Ok(SeaFile {
            header: header.clone(),
            cbr_encoder: Some(cbr_encoder),
            vbr_encoder: Some(vbr_encoder),
            encoder_settings: Some(encoder_settings.clone()),
            dequant_tab: SeaDequantTab::init(encoder_settings.scale_factor_bits as usize),
        })
    }

    pub fn from_reader<R: io::Read>(mut reader: &mut R) -> Result<Self, SeaError> {
        let header = SeaFileHeader::from_reader(&mut reader)?;

        Ok(SeaFile {
            header,
            cbr_encoder: None,
            vbr_encoder: None,
            encoder_settings: None,
            dequant_tab: SeaDequantTab::init(0),
        })
    }

    pub fn make_chunk(&mut self, samples: &[i16]) -> Result<Vec<u8>, SeaError> {
        let encoder_settings = self.encoder_settings.as_ref().unwrap();
        let vbr_encoder = self.vbr_encoder.as_mut().unwrap();
        let cbr_encoder = self.cbr_encoder.as_mut().unwrap();

        let initial_lms = match encoder_settings.vbr {
            true => vbr_encoder.lms.clone(),
            false => cbr_encoder.lms.clone(),
        };

        let encoded = match encoder_settings.vbr {
            true => vbr_encoder.encode(samples, &mut self.dequant_tab),
            false => cbr_encoder.encode(samples, &mut self.dequant_tab),
        };

        let chunk = SeaChunk::new(
            &self.header,
            &initial_lms,
            &encoder_settings,
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

    pub fn chunk_from_reader<R: io::Read>(
        &mut self,
        reader: &mut R,
        remaining_frames: Option<usize>,
    ) -> Result<Option<SeaChunk>, SeaError> {
        let encoded = read_max_or_zero(reader, self.header.chunk_size as usize)?;
        if encoded.len() == 0 {
            return Ok(None);
        }

        let chunk = SeaChunk::from_slice(
            &encoded,
            &self.header,
            remaining_frames,
            &mut self.dequant_tab,
        );
        match chunk {
            Ok(chunk) => Ok(Some(chunk)),
            Err(err) => Err(err),
        }
    }
}
