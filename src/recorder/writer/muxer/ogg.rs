use crate::recorder::writer::muxer::crc::vorbis_crc32;

/// The maximum number of payload bytes in an Ogg page.
///
const MAX_PAYLOAD_PER_PAGE: usize = 65_025;
/// Maximum number of packets in a page.
pub const MAX_SEGMENTS_PER_FRAME: usize = 255;
const MAX_SEGMENT_SIZE: u16 = 255;

#[derive(Debug)]
pub struct OggSegments {
    lacings: Vec<u8>,
    total_size: u16,
}

impl OggSegments {
    pub fn new() -> Self {
        Self {
            lacings: Vec::new(),
            total_size: 0,
        }
    }

    pub fn clear(&mut self) {
        self.lacings.clear();
        self.total_size = 0;
    }

    /// Adds a segment to the segment table.
    /// Returns the length of leftover bytes that do not fit into the packet.
    pub fn push_packet(&mut self, length: usize) -> Option<u16> {
        let full_segments = length as u16 / MAX_SEGMENT_SIZE;
        let leftover_bytes = length as u16 % MAX_SEGMENT_SIZE;

        let mut full_segments_left = full_segments;

        while full_segments_left > 0 {
            if self.lacings.len() == MAX_PAYLOAD_PER_PAGE {
                let overflow_bytes = (full_segments_left * MAX_SEGMENT_SIZE) + leftover_bytes;
                return Some(overflow_bytes);
            }

            self.lacings.push(MAX_SEGMENT_SIZE as u8);
            self.total_size += MAX_SEGMENT_SIZE;
            full_segments_left -= 1;
        }

        if self.lacings.len() == MAX_PAYLOAD_PER_PAGE {
            return Some(leftover_bytes);
        }

        self.lacings.push(leftover_bytes as u8);
        self.total_size += leftover_bytes;

        None
    }

    /// Returns Some(x) where x is the number of bytes that would overflow to the following
    /// packet, or None if the packet fits entirely within the current page.
    pub fn would_split(&self, length: usize) -> Option<u16> {
        let needed_full_segments = length as u16 / MAX_SEGMENT_SIZE;
        let needed_leftover_bytes = length as u16 % MAX_SEGMENT_SIZE;
        let needed_total_segments = needed_full_segments + 1;

        let current_segments = self.lacings.len() as u16;

        let segment_overage = (current_segments + needed_total_segments).saturating_sub(MAX_SEGMENTS_PER_FRAME as u16);
        if segment_overage > 0 {
            Some( ((segment_overage - 1) * MAX_SEGMENT_SIZE) + needed_leftover_bytes )
        } else {
            None
        }
    }
}

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
}

impl OggHeader {
    pub fn build_page(&self, segments: &OggSegments, payload: &[u8]) -> Option<Vec<u8>> {
        let mut buffer= Vec::new();

        if payload.len() > MAX_PAYLOAD_PER_PAGE {
            error!("Payload too large: {} bytes", payload.len());
            return None;
        }

        if segments.lacings.len() > MAX_SEGMENTS_PER_FRAME {
            error!("Too many segments: {}", segments.lacings.len());
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

        buffer.push(segments.lacings.len() as u8); // 26
        buffer.extend_from_slice(segments.lacings.as_slice());

        buffer.extend_from_slice(payload);

        update_crc(buffer.as_mut_slice());

        Some(buffer)
    }
}

fn update_crc(data: &mut [u8]) {
    let crc = vorbis_crc32(data);
    data[22..=25].copy_from_slice(&crc.to_le_bytes());
}