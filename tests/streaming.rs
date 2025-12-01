use std::{
    cell::RefCell,
    io::{self, Read, Write},
    rc::Rc,
};

use helpers::{encode_decode, gen_test_signal, TEST_SAMPLE_RATE};
use sea_codec::{
    decoder::SeaDecoder,
    encoder::{EncoderSettings, SeaEncoder},
};

extern crate sea_codec;

mod helpers;

#[derive(Clone)]
struct SharedBuffer {
    buffer: Rc<RefCell<Vec<u8>>>,
}

impl SharedBuffer {
    fn new(capacity: usize) -> Self {
        SharedBuffer {
            buffer: Rc::new(RefCell::new(Vec::with_capacity(capacity))),
        }
    }
}

impl Write for SharedBuffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer.borrow_mut().write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.buffer.borrow_mut().flush()
    }
}

impl Read for SharedBuffer {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut vec = self.buffer.borrow_mut();
        let amount = buf.len().min(vec.len());
        buf[..amount].copy_from_slice(&vec[..amount]);
        vec.drain(..amount);
        Ok(amount)
    }
}

#[cfg(feature = "std")]
#[test]
fn streaming() {
    let channels = 1;
    let input_samples = gen_test_signal(channels, TEST_SAMPLE_RATE as usize);

    let reference_samples = encode_decode(
        &input_samples,
        TEST_SAMPLE_RATE,
        channels,
        EncoderSettings::default(),
    );

    let sea_encoded = SharedBuffer::new(input_samples.len());
    let mut sea_encoded_clone = sea_encoded.clone();

    let mut sea_encoder = SeaEncoder::from_slice(
        channels as u8,
        TEST_SAMPLE_RATE,
        None,
        EncoderSettings::default(),
        &input_samples,
    )
    .unwrap();

    // need to encode first frame to get the header
    sea_encoder.encode_frame(&mut sea_encoded_clone).unwrap();

    let mut sea_decoded = Vec::<i16>::with_capacity(input_samples.len() * 2);
    let sea_encoded_dec_clone = sea_encoded.clone();
    let mut sea_decoder = SeaDecoder::from_reader(sea_encoded_dec_clone).unwrap();

    for _ in 0..3 {
        sea_encoder.encode_frame(&mut sea_encoded_clone).unwrap();
        sea_decoder.decode_frame(&mut sea_decoded).unwrap();
    }

    assert!(!sea_decoded.is_empty());
    assert_eq!(
        reference_samples.decoded[..sea_decoded.len()],
        sea_decoded[..]
    );
}
