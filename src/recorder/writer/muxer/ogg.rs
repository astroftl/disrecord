use crate::recorder::writer::muxer::crc::vorbis_crc32;

/// Not technically correct, but makes my life easier.
const MAX_PAYLOAD: usize = 65_025;
/// Maximum size of a worst-case Ogg header.
const MAX_HEADER: usize = 282;
/// Maximum number of packets in a page.
pub const MAX_PACKETS_PER_PAGE: usize = 255;

#[derive(Debug)]
pub struct OggHeader {
    /// True if page contains the continuation of a packet from the previous page.
    pub continuation: bool,
    /// True if the first page of a stream.
    pub begin_stream: bool,
    /// True if the last page of a stream.
    pub end_stream: bool,
    pub granule: u64,
    /// Random number per stream.
    pub serial: u32,
    pub sequence: u32,
    pub segment_lengths: Vec<u16>
}

impl OggHeader {
    pub fn build_page(&self, payload: &[u8]) -> Option<Vec<u8>> {
        let mut buffer= Vec::new();

        if payload.len() > MAX_PAYLOAD {
            error!("Payload too large: {} bytes", payload.len());
            return None;
        }

        if self.segment_lengths.len() > 255 {
            error!("Too many segments: {}", self.segment_lengths.len());
            return None;
        }

        buffer.extend_from_slice(&[b'O', b'g', b'g', b'S']); // 0-3
        buffer.push(0); // Version, 4

        let mut header_type = 0;
        if self.continuation {
            header_type |= 0x01;
        }
        if self.begin_stream {
            header_type |= 0x02;
        }
        if self.end_stream {
            header_type |= 0x04;
        }
        buffer.push(header_type); // 5

        buffer.extend_from_slice(&self.granule.to_le_bytes()); // 6-13
        buffer.extend_from_slice(&self.serial.to_le_bytes()); // 14-17
        buffer.extend_from_slice(&self.sequence.to_le_bytes()); // 18-21

        // buffer[22..=25] is the CRC checksum, to be calculated after.
        buffer.extend_from_slice(&[0, 0, 0, 0]);

        let mut lacings: Vec<u8> = Vec::new();
        for segment_length in &self.segment_lengths {
            let mut segment_length = segment_length.clone();

            while segment_length > 255 {
                lacings.push(255);
                segment_length -= 255;
            }

            if segment_length == 255 {
                lacings.push(0);
            } else {
                lacings.push(segment_length as u8);
            }
        }

        buffer.push(lacings.len() as u8); // 26
        buffer.extend_from_slice(lacings.as_slice());

        buffer.extend_from_slice(payload);

        update_crc(buffer.as_mut_slice());

        Some(buffer)
    }
}

fn update_crc(data: &mut [u8]) {
    let crc = vorbis_crc32(data);
    data[22..=25].copy_from_slice(&crc.to_le_bytes());
}