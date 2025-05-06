pub const PRESKIP_DEFAULT: u16 = 3840;
pub const GRANULE_ENTIRE_PACKET: u64 = u64::MAX;

pub struct ChannelMappingTable {
    pub stream_count: u8,
    pub coupled_count: u8,
    pub channel_mapping: Vec<u8>,
}

pub enum MappingFamily {
    Rtp,
    Vorbis(ChannelMappingTable),
    Unidentified(ChannelMappingTable),
    Undefined(ChannelMappingTable),
}

impl MappingFamily {
    fn byte(&self) -> u8 {
        match self {
            MappingFamily::Rtp => 0,
            MappingFamily::Vorbis(_) => 1,
            MappingFamily::Unidentified(_) => 255,
            MappingFamily::Undefined(_) => 255,
        }
    }
}

pub struct IdHeader {
    pub channel_count: u8,
    pub preskip: u16,
    pub input_sample_rate: u32,
    pub gain: u16,
    pub mapping_family: MappingFamily,
}

impl IdHeader {
    pub fn build(&self) -> Vec<u8> {
        let mut header = Vec::new();

        header.extend_from_slice(&[b'O', b'p', b'u', b's', b'H', b'e', b'a', b'd']);
        header.push(1); // Version
        header.extend_from_slice(&self.channel_count.to_le_bytes());
        header.extend_from_slice(&self.preskip.to_le_bytes());
        header.extend_from_slice(&self.input_sample_rate.to_le_bytes());
        header.extend_from_slice(&self.gain.to_le_bytes());

        match &self.mapping_family {
            MappingFamily::Rtp => {
                header.push(self.mapping_family.byte());
            }
            MappingFamily::Vorbis(table)
            | MappingFamily::Unidentified(table)
            | MappingFamily::Undefined(table) => {
                header.push(self.mapping_family.byte());
                header.push(table.stream_count);
                header.push(table.coupled_count);
                header.extend_from_slice(table.channel_mapping.as_slice());
            }
        }

        header
    }
}

pub struct CommentHeader {
    pub vendor: String,
    pub comments: Vec<String>,
}

impl CommentHeader {
    pub fn build(&self) -> Vec<u8> {
        let mut header = Vec::new();

        header.extend_from_slice(&[b'O', b'p', b'u', b's', b'T', b'a', b'g', b's']);

        header.extend_from_slice((self.vendor.len() as u32).to_le_bytes().as_slice());
        header.extend_from_slice(self.vendor.as_bytes());

        for comment in &self.comments {
            header.extend_from_slice((comment.len() as u32).to_le_bytes().as_slice());
            header.extend_from_slice(comment.as_bytes());
        }

        header
    }
}