use alloc::vec::Vec;

use crate::{
    codec::{
        common::SeaError,
        file::{SeaFile, SeaFileHeader},
    },
    cursor::Cursor,
};

pub struct SeaDecoder<'inp> {
    cursor: Cursor<'inp>,
    file: SeaFile,
    frames_read: usize,
}

impl<'inp> SeaDecoder<'inp> {
    #[cfg(feature = "std")]
    pub fn from_reader<R: std::io::Read + 'inp>(reader: R) -> Result<Self, SeaError> {
        let mut cursor = Cursor::from_reader(reader);

        let file = SeaFile::from_reader(&mut cursor)?;

        Ok(Self {
            cursor,
            file,
            frames_read: 0,
        })
    }

    pub fn from_slice(data: &'inp [u8]) -> Result<Self, SeaError> {
        let mut cursor = Cursor::from_slice(data);

        let file = SeaFile::from_reader(&mut cursor)?;

        Ok(Self {
            cursor,
            file,
            frames_read: 0,
        })
    }

    pub fn decode_frame(&mut self) -> Result<Option<Vec<i16>>, SeaError> {
        if self.file.header.total_frames != 0
            && (self.file.header.total_frames as usize) <= self.frames_read
        {
            return Ok(None);
        }

        let remaining_frames = if self.file.header.total_frames > 0 {
            Some(self.file.header.total_frames as usize - self.frames_read)
        } else {
            None
        };

        let reader_res = self
            .file
            .samples_from_reader(&mut self.cursor, remaining_frames)?;

        match reader_res {
            Some(samples) => {
                self.frames_read += samples.len() / self.file.header.channels as usize;
                Ok(Some(samples))
            }
            None => Ok(None),
        }
    }

    pub fn get_header(&self) -> SeaFileHeader {
        self.file.header.clone()
    }
}
