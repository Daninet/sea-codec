use alloc::{rc::Rc, string::String};

use crate::codec::{
    common::SeaError,
    file::{SeaFile, SeaFileHeader},
};

pub enum SeaEncoderState {
    Start,
    WritingFrames,
    Finished,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EncoderSettings {
    pub scale_factor_bits: u8,
    pub scale_factor_frames: u8,
    pub residual_bits: f32, // 1-8
    pub frames_per_chunk: u16,
    pub vbr: bool,
}

impl Default for EncoderSettings {
    fn default() -> Self {
        Self {
            frames_per_chunk: 5120,
            scale_factor_bits: 4,
            scale_factor_frames: 20,
            residual_bits: 3.0,
            vbr: false,
        }
    }
}

trait InternalWrite {
    fn write_all(&mut self, buf: &[u8]) -> Result<(), SeaError>;
}

#[cfg(feature = "std")]
impl<W: std::io::Write> InternalWrite for W {
    fn write_all(&mut self, buf: &[u8]) -> Result<(), SeaError> {
        Ok(self.write_all(buf)?)
    }
}

#[cfg(not(feature = "std"))]
impl InternalWrite for &mut alloc::vec::Vec<u8> {
    fn write_all(&mut self, buf: &[u8]) -> Result<(), SeaError> {
        self.extend_from_slice(buf);
        Ok(())
    }
}

pub struct SeaEncoder<'inp> {
    data: &'inp [i16],
    file: SeaFile,
    state: SeaEncoderState,
    written_frames: u32,
    passed_total_frames: Option<u32>,
}

impl<'inp> SeaEncoder<'inp> {
    pub fn from_slice(
        channels: u8,
        sample_rate: u32,
        total_frames: Option<u32>,
        settings: EncoderSettings,
        data: &'inp [i16],
    ) -> Result<Self, SeaError> {
        let header = SeaFileHeader {
            version: 1,
            channels,
            chunk_size: 0, // will be set later by the first chunk
            frames_per_chunk: settings.frames_per_chunk,
            sample_rate,
            total_frames: total_frames.unwrap_or(0),
            metadata: Rc::new(String::new()),
        };

        let file = SeaFile::new(header, &settings)?;

        let state = SeaEncoderState::Start;

        Ok(SeaEncoder {
            file,
            state,
            data,
            written_frames: 0,
            passed_total_frames: total_frames,
        })
    }

    fn read_samples(&mut self, max_sample_count: usize) -> Result<&'inp [i16], SeaError> {
        let max_to_read = self.data.len().min(max_sample_count);

        if max_to_read == 0 {
            return Ok(&self.data[..0]);
        }

        if !max_to_read.is_multiple_of(self.file.header.channels as usize) {
            return Err(SeaError::EndOfFile);
        }

        let (samples, new_data) = self.data.split_at(max_to_read);
        self.data = new_data;

        Ok(samples)
    }

    #[cfg(feature = "std")]
    pub fn encode_frame(&mut self, writer: impl std::io::Write) -> Result<bool, SeaError> {
        self.encode_frame_inner(writer)
    }

    #[cfg(not(feature = "std"))]
    pub fn encode_frame(&mut self, writer: &mut alloc::vec::Vec<u8>) -> Result<bool, SeaError> {
        self.encode_frame_inner(writer)
    }

    fn encode_frame_inner<W: InternalWrite>(&mut self, mut writer: W) -> Result<bool, SeaError> {
        if matches!(self.state, SeaEncoderState::Finished) {
            return Err(SeaError::EncoderClosed);
        }

        if matches!(self.state, SeaEncoderState::Start) {
            if let Some(total_frames) = self.passed_total_frames {
                if total_frames == 0 {
                    writer.write_all(&self.file.header.serialize())?;
                    self.state = SeaEncoderState::WritingFrames;
                }
            }
        }

        let channels = self.file.header.channels;
        let frames = if self.file.header.total_frames > 0 {
            (self.file.header.frames_per_chunk as usize)
                .min(self.file.header.total_frames as usize - self.written_frames as usize)
        } else {
            self.file.header.frames_per_chunk as usize
        };

        let full_size_samples =
            self.file.header.frames_per_chunk as usize * self.file.header.channels as usize;
        let samples_to_read = frames * channels as usize;
        let samples = self.read_samples(samples_to_read)?;
        let eof: bool = samples.is_empty() || samples.len() < full_size_samples;

        if !samples.is_empty() {
            let encoded_chunk = self.file.make_chunk(samples)?;

            if eof {
                assert!(encoded_chunk.len() <= self.file.header.chunk_size as usize);
            } else {
                assert_eq!(encoded_chunk.len(), self.file.header.chunk_size as usize);
            }

            // we need to write file header after the first chunk is generated
            if matches!(self.state, SeaEncoderState::Start) {
                writer.write_all(&self.file.header.serialize())?;
                self.state = SeaEncoderState::WritingFrames;
            }

            writer.write_all(&encoded_chunk)?;
            self.written_frames += frames as u32;
        }

        if eof {
            self.state = SeaEncoderState::Finished;
        }

        Ok(!eof)
    }

    pub fn finalize(&mut self) -> Result<(), SeaError> {
        self.state = SeaEncoderState::Finished;
        Ok(())
    }
}
