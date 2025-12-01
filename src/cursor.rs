use crate::codec::common::SeaError;

pub enum Cursor<'inp> {
    Slice(&'inp [u8]),
    #[cfg(feature = "std")]
    Reader(Box<dyn std::io::Read + 'inp>),
}
#[cfg(feature = "std")]
impl<'inp> Cursor<'inp> {
    pub(crate) fn from_reader<R: std::io::Read + 'inp>(reader: R) -> Self {
        Self::Reader(Box::new(reader))
    }
}

impl<'inp> Cursor<'inp> {
    pub(crate) fn from_slice(data: &'inp [u8]) -> Self {
        Self::Slice(data)
    }

    pub fn read_exact(&mut self, result: &mut [u8]) -> Result<(), SeaError> {
        match self {
            Cursor::Slice(data) => {
                if data.len() < result.len() {
                    Err(SeaError::EndOfFile)
                } else {
                    let (r, remaining) = data.split_at(result.len());
                    *data = remaining;
                    result.copy_from_slice(r);
                    Ok(())
                }
            }
            #[cfg(feature = "std")]
            Cursor::Reader(reader) => Ok(reader.read_exact(result)?),
        }
    }

    pub fn read(&mut self, result: &mut [u8]) -> Result<usize, SeaError> {
        match self {
            Cursor::Slice(data) => {
                let to_read = result.len().min(data.len());
                self.read_exact(&mut result[..to_read])?;
                Ok(to_read)
            }
            #[cfg(feature = "std")]
            Cursor::Reader(reader) => Ok(reader.read(result)?),
        }
    }
}
