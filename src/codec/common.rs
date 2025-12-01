use alloc::vec;
use alloc::vec::Vec;

use crate::cursor::Cursor;

pub const SEAC_MAGIC: u32 = u32::from_be_bytes(*b"seac"); // 0x73 0x65 0x61 0x63

#[inline(always)]
pub fn clamp_i16(v: i32) -> i16 {
    v.clamp(i16::MIN as i32, i16::MAX as i32) as i16
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum SeaResidualSize {
    One = 1,
    Two = 2,
    Three = 3,
    Four = 4,
    Five = 5,
    Six = 6,
    Seven = 7,
    Eight = 8,
}

impl SeaResidualSize {
    #[inline(always)]
    pub fn from(len: u8) -> Self {
        match len {
            1 => SeaResidualSize::One,
            2 => SeaResidualSize::Two,
            3 => SeaResidualSize::Three,
            4 => SeaResidualSize::Four,
            5 => SeaResidualSize::Five,
            6 => SeaResidualSize::Six,
            7 => SeaResidualSize::Seven,
            8 => SeaResidualSize::Eight,
            _ => panic!("Invalid residual length"),
        }
    }

    #[inline(always)]
    pub fn to_binary_combinations(self) -> usize {
        match self {
            SeaResidualSize::One => 2,
            SeaResidualSize::Two => 4,
            SeaResidualSize::Three => 8,
            SeaResidualSize::Four => 16,
            SeaResidualSize::Five => 32,
            SeaResidualSize::Six => 64,
            SeaResidualSize::Seven => 128,
            SeaResidualSize::Eight => 256,
        }
    }
}

#[derive(Debug)]
pub enum SeaError {
    ReadError,
    InvalidParameters,
    InvalidFile,
    InvalidFrame,
    EncoderClosed,
    UnsupportedVersion,
    TooManyFrames,
    MetadataTooLarge,
    EndOfFile,
    #[cfg(feature = "std")]
    IoError(std::io::Error),
}

#[cfg(feature = "std")]
impl From<std::io::Error> for SeaError {
    fn from(error: std::io::Error) -> Self {
        SeaError::IoError(error)
    }
}

#[inline(always)]
pub fn read_bytes<const BYTES: usize>(reader: &mut Cursor) -> Result<[u8; BYTES], SeaError> {
    let mut buf = [0_u8; BYTES];
    reader.read_exact(&mut buf)?;
    Ok(buf)
}

#[inline(always)]
pub fn read_u8(reader: &mut Cursor) -> Result<u8, SeaError> {
    let data: [u8; 1] = read_bytes(reader)?;
    Ok(data[0])
}

#[inline(always)]
pub fn read_u16_le(reader: &mut Cursor) -> Result<u16, SeaError> {
    let data = read_bytes(reader)?;
    Ok(u16::from_le_bytes(data))
}

#[inline(always)]
pub fn read_u32_be(reader: &mut Cursor) -> Result<u32, SeaError> {
    let data = read_bytes(reader)?;
    Ok(u32::from_be_bytes(data))
}

#[inline(always)]
pub fn read_u32_le(reader: &mut Cursor) -> Result<u32, SeaError> {
    let data = read_bytes(reader)?;
    Ok(u32::from_le_bytes(data))
}

pub fn read_max_or_zero(reader: &mut Cursor, at_least_bytes: usize) -> Result<Vec<u8>, SeaError> {
    let mut buffer = vec![0u8; at_least_bytes];
    let mut total_bytes_read = 0;

    while total_bytes_read < at_least_bytes {
        let bytes_read = reader.read(&mut buffer[total_bytes_read..])?;

        // EOF
        if bytes_read == 0 {
            break;
        }

        total_bytes_read += bytes_read;
    }

    if total_bytes_read == 0 {
        return Ok(Vec::new());
    }

    Ok(buffer[..total_bytes_read].to_vec())
}

#[derive(Debug)]
pub struct EncodedSamples {
    pub scale_factors: Vec<u8>,
    pub residuals: Vec<u8>,
    pub residual_bits: Vec<u8>,
}

pub trait SeaEncoderTrait {
    fn encode(&mut self, input_slice: &[i16]) -> EncodedSamples;
}
