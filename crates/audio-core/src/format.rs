//! Canonical audio representations and format definitions.

pub const CANONICAL_SAMPLE_RATE: u32 = 48_000;
pub const CANONICAL_CHANNELS: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFormat {
    U8,
    I16,
    I24,
    I32,
    F32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioSpec {
    pub sample_rate: u32,
    pub channels: u16,
    pub sample_format: SampleFormat,
}

impl AudioSpec {
    pub const CANONICAL: Self = Self {
        sample_rate: CANONICAL_SAMPLE_RATE,
        channels: CANONICAL_CHANNELS,
        sample_format: SampleFormat::F32,
    };
}
