#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpusToc {
    pub mode: OpusMode,
    pub bandwidth: Bandwidth,
    pub frame_size: FrameSize,
    pub stereo: bool,
    pub frame_count: FrameCount,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpusMode {
    Silk,
    Hybrid,
    Celt,
}

impl OpusMode {
    const fn from(value: u8) -> Self {
        match value {
            0..=11 => OpusMode::Silk,
            12..=15 => OpusMode::Hybrid,
            16..=31 => OpusMode::Celt,
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bandwidth {
    Narrowband,
    Mediumband,
    Wideband,
    Superwideband,
    Fullband,
}

impl Bandwidth {
    pub const fn sample_rate(self) -> u32 {
        match self {
            Bandwidth::Narrowband => 8_000,
            Bandwidth::Mediumband => 12_000,
            Bandwidth::Wideband => 16_000,
            Bandwidth::Superwideband => 24_000,
            Bandwidth::Fullband => 48_000,
        }
    }

    const fn from(value: u8) -> Self {
        match value {
            0..=3 | 16..=19 => Bandwidth::Narrowband,
            4..=7 => Bandwidth::Mediumband,
            8..=11 | 20..=23 => Bandwidth::Wideband,
            12..=13 | 24..=27 => Bandwidth::Superwideband,
            14..=15 | 28..=31 => Bandwidth::Fullband,
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameSize {
    Ms2_5,
    Ms5,
    Ms10,
    Ms20,
    Ms40,
    Ms60,
}

impl FrameSize {
    fn to_10_000_factor(self) -> usize {
        match self {
            FrameSize::Ms2_5 => 25,
            FrameSize::Ms5 => 50,
            FrameSize::Ms10 => 100,
            FrameSize::Ms20 => 200,
            FrameSize::Ms40 => 400,
            FrameSize::Ms60 => 600,
        }
    }

    const fn from(value: u8) -> Self {
        match value {
            0..=3 => {
                let base = 0;
                match value - base {
                    0 =>  FrameSize::Ms10,
                    1 =>  FrameSize::Ms20,
                    2 =>  FrameSize::Ms40,
                    3 =>  FrameSize::Ms60,
                    _ => unreachable!(),
                }
            }
            4..=7 => {
                let base = 4;
                match value - base {
                    0 =>  FrameSize::Ms10,
                    1 =>  FrameSize::Ms20,
                    2 =>  FrameSize::Ms40,
                    3 =>  FrameSize::Ms60,
                    _ => unreachable!(),
                }
            }
            8..=11 => {
                let base = 8;
                match value - base {
                    0 =>  FrameSize::Ms10,
                    1 =>  FrameSize::Ms20,
                    2 =>  FrameSize::Ms40,
                    3 =>  FrameSize::Ms60,
                    _ => unreachable!(),
                }
            }
            12..=13 => {
                let base = 12;
                match value - base {
                    0 =>  FrameSize::Ms10,
                    1 =>  FrameSize::Ms20,
                    _ => unreachable!(),
                }
            }
            14..=15 => {
                let base = 14;
                match value - base {
                    0 =>  FrameSize::Ms10,
                    1 =>  FrameSize::Ms20,
                    _ => unreachable!(),
                }
            }
            16..=19 => {
                let base = 16;
                match value - base {
                    0 =>  FrameSize::Ms2_5,
                    1 =>  FrameSize::Ms5,
                    2 =>  FrameSize::Ms10,
                    3 =>  FrameSize::Ms20,
                    _ => unreachable!(),
                }
            }
            20..=23 => {
                let base = 20;
                match value - base {
                    0 =>  FrameSize::Ms2_5,
                    1 =>  FrameSize::Ms5,
                    2 =>  FrameSize::Ms10,
                    3 =>  FrameSize::Ms20,
                    _ => unreachable!(),
                }
            }
            24..=27 => {
                let base = 24;
                match value - base {
                    0 =>  FrameSize::Ms2_5,
                    1 =>  FrameSize::Ms5,
                    2 =>  FrameSize::Ms10,
                    3 =>  FrameSize::Ms20,
                    _ => unreachable!(),
                }
            }
            28..=31 => {
                let base = 28;
                match value - base {
                    0 =>  FrameSize::Ms2_5,
                    1 =>  FrameSize::Ms5,
                    2 =>  FrameSize::Ms10,
                    3 =>  FrameSize::Ms20,
                    _ => unreachable!(),
                }
            }
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameCount {
    One,
    TwoEqual,
    TwoDifferent,
    Arbitrary,
}

impl FrameCount {
    const fn from(value: u8) -> Self {
        match value & 0b11 {
            0b00 => FrameCount::One,
            0b01 => FrameCount::TwoEqual,
            0b10 => FrameCount::TwoDifferent,
            0b11 => FrameCount::Arbitrary,
            _ => unreachable!(),
        }
    }
}

impl OpusToc {
    pub fn sample_count(&self) -> usize {
        let sample_rate: u32 = self.bandwidth.sample_rate();
        let frame_length = self.frame_size.to_10_000_factor();

        ((sample_rate as usize) * frame_length) / 10_000
    }

    pub const fn from(value: u8) -> Self {
        let config = value >> 3;
        let frame_count_bits = value & 0b0000_0011;

        let stereo = (value & 0b0000_0100) != 0;

        let mode = OpusMode::from(config);
        let bandwidth = Bandwidth::from(config);
        let frame_size = FrameSize::from(config);

        let frame_count = FrameCount::from(frame_count_bits);

        Self {
            mode,
            bandwidth,
            frame_size,
            stereo,
            frame_count,
        }
    }
}