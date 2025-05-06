use std::cmp::min;
use std::path::PathBuf;
use std::sync::Mutex;
use rand::Rng;
use serenity::all::{GuildId, UserId};
use songbird::packet::wrap::Wrap32;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex as AsyncMutex;
use crate::recorder::writer::muxer::ogg::{OggHeader, MAX_PACKETS_PER_PAGE};
use crate::recorder::writer::muxer::ogg_opus::{CommentHeader, IdHeader, MappingFamily, PRESKIP_DEFAULT};
use crate::recorder::writer::muxer::opus_toc::{Bandwidth, FrameSize, OpusToc};
use crate::recorder::writer::RtpUpdate;

const DUMP_PACKETS: usize = 200;
const DUMP_LENGTH: usize = 50_000;

const SILENCE_PACKET: [u8; 3] = [0xF8, 0xFF, 0xFE];

#[derive(Debug)]
pub struct PacketBuffer {
    pub opus: Vec<u8>,
    pub lengths: Vec<u16>,
    pub tocs: Vec<OpusToc>,
    pub total_length: usize,
    pub first_timestamp: Option<Wrap32>,
    pub last_timestamp: Option<Wrap32>,
}

impl PacketBuffer {
    pub fn new() -> Self {
        Self {
            opus: vec![],
            lengths: vec![],
            tocs: vec![],
            total_length: 0,
            first_timestamp: None,
            last_timestamp: None,
        }
    }

    pub fn clear(&mut self) {
        self.opus.clear();
        self.lengths.clear();
        self.tocs.clear();
        self.total_length = 0;
        self.first_timestamp = None;
        self.last_timestamp = None;
    }
}

#[derive(Debug)]
pub struct OpusState {
    sequence: u32,
    recording_first_timestamp: Wrap32,
    bandwidth: Bandwidth,
    started: bool,
    packet_buffer: PacketBuffer,
}

#[derive(Debug)]
pub struct StreamWriter {
    guild_id: GuildId,
    user_id: UserId,
    serial: u32,
    state: Mutex<OpusState>,
    file: AsyncMutex<File>,
    file_path: PathBuf,
}

impl StreamWriter {
    pub async fn new(guild_id: GuildId, user_id: UserId, output_dir: PathBuf) -> Option<Self> {
        let rand_serial = rand::rng().random::<u32>();

        if let Err(e) = tokio::fs::create_dir_all(output_dir.clone()).await {
            error!("[{guild_id}] <{user_id}> Failed to create parent directory: {e:?}");
            return None;
        }

        let file_path = output_dir.join(user_id.to_string() + ".opus");

        debug!("[{guild_id}] <{user_id}> Creating output file: {}", file_path.display());

        match File::create(&file_path).await {
            Ok(file) => {
                let state = OpusState {
                    sequence: 0,
                    recording_first_timestamp: Wrap32::new(0),
                    bandwidth: Bandwidth::Fullband,
                    started: false,
                    packet_buffer: PacketBuffer::new(),
                };

                Some(Self {
                    guild_id,
                    user_id,
                    serial: rand_serial,
                    state: Mutex::new(state),
                    file:  AsyncMutex::new(file),
                    file_path,
                })
            }
            Err(e) => {
                error!("[{guild_id}] <{user_id}> Failed to create new file {}: {e:?}", file_path.display());
                None
            }
        }
    }

    pub async fn start(&self, bandwidth: Bandwidth, first_timestamp: Wrap32) {
        debug!("[{}] <{}> Starting file: {}", self.guild_id, self.user_id, self.file_path.display());

        let opus_id_header = IdHeader {
            channel_count: 2,
            preskip: PRESKIP_DEFAULT,
            input_sample_rate: bandwidth.sample_rate(),
            gain: 0,
            mapping_family: MappingFamily::Rtp,
        };

        let opus_id_data = opus_id_header.build();

        let id_page_header = OggHeader {
            continuation: false,
            begin_stream: true,
            end_stream: false,
            granule: 0,
            serial: self.serial,
            sequence: 0,
            segment_lengths: vec![opus_id_data.len() as u16],
        };

        debug!("[{}] <{}> ID page header: {id_page_header:?}", self.guild_id, self.user_id);

        let id_page = match id_page_header.build_page(opus_id_data.as_slice()) {
            Some(x) => x,
            None => {
                error!("[{}] <{}> Failed to build Opus ID page!", self.guild_id, self.user_id);
                return;
            }
        };

        {
            let mut file =  self.file.lock().await;
            file.write_all(id_page.as_slice()).await.unwrap();
        }

        let opus_comment_header = CommentHeader {
            vendor: "disrecord".to_string(),
            comments: vec![]
        };

        let opus_comment_data = opus_comment_header.build();

        let comment_page_header = OggHeader {
            continuation: false,
            begin_stream: false,
            end_stream: false,
            granule: 0,
            serial: self.serial,
            sequence: 1,
            segment_lengths: vec![opus_comment_data.len() as u16],
        };

        debug!("[{}] <{}> Comment page header: {comment_page_header:?}", self.guild_id, self.user_id);

        let comment_page = match comment_page_header.build_page(opus_comment_data.as_slice()) {
            Some(x) => x,
            None => {
                error!("[{}] <{}> Failed to build Opus ID page!", self.guild_id, self.user_id);
                return;
            }
        };

        {
            let mut file =  self.file.lock().await;
            file.write_all(comment_page.as_slice()).await.unwrap();
        }

        {
            let mut state = self.state.lock().unwrap();
            state.started = true;
            state.sequence = 2;
            state.recording_first_timestamp = first_timestamp;
            state.bandwidth = bandwidth;
        }
    }

    async fn fill_silence(&self, samples: usize, mut last_granule: u32) {
        let bandwidth = self.state.lock().unwrap().bandwidth;
        let mut sequence = self.state.lock().unwrap().sequence;

        let silence_toc = OpusToc::from(SILENCE_PACKET[0]);
        let ms20_divider = FrameSize::Ms20.samples(bandwidth);

        let mut samples_left = samples;

        let ms20_packets = samples_left / ms20_divider;
        samples_left = samples_left % ms20_divider;

        trace!("[{}] <{}> Filling gaps with silence packets: [20ms * {ms20_packets}]", self.guild_id, self.user_id);
        if samples_left != 0 {
            warn!("[{}] <{}> We have leftover samples: {samples_left}", self.guild_id, self.user_id);
        }

        let mut packets_left = ms20_packets;

        while packets_left > 0 {
            let page_packets = min(packets_left, MAX_PACKETS_PER_PAGE);

            let mut opus_data: Vec<u8> = Vec::new();
            let mut segment_lengths: Vec<u16> = Vec::new();
            for _ in 0..page_packets {
                opus_data.extend_from_slice(&SILENCE_PACKET);
                segment_lengths.push(SILENCE_PACKET.len() as u16);
                last_granule += silence_toc.sample_count() as u32;
            }

            let page_header = OggHeader {
                continuation: false,
                begin_stream: false,
                end_stream: false,
                granule: last_granule as u64,
                serial: self.serial,
                sequence,
                segment_lengths,
            };

            packets_left -= page_packets;
            sequence += 1;

            let page_data = page_header.build_page(opus_data.as_slice()).unwrap();

            {
                let mut file =  self.file.lock().await;
                file.write_all(page_data.as_slice()).await.unwrap();
            }
        }
    }

    async fn dump(&self, finalize: bool) {
        let started = self.state.lock().unwrap().started;
        if !started {
            let (bandwidth, first_timestamp) = {
                let state = self.state.lock().unwrap();
                let packet_buffer = &state.packet_buffer;

                let bandwidth = match packet_buffer.tocs.first() {
                    None => Bandwidth::Fullband,
                    Some(toc) => toc.bandwidth,
                };

                (bandwidth, packet_buffer.first_timestamp.unwrap())
            };

            self.start(bandwidth, first_timestamp).await;
        }

        let page_data = {
            let mut state = self.state.lock().unwrap();

            let (sample_rate, sample_count) = match state.packet_buffer.tocs.last() {
                None => (0, 0),
                Some(x) => (x.bandwidth.sample_rate(), x.sample_count())
            };

            let granule = match state.packet_buffer.last_timestamp {
                None => 0,
                Some(last_timestamp) => {
                    (last_timestamp - state.recording_first_timestamp.into()).0.0 + sample_count as u32
                }
            };

            trace!("[{}] <{}> Dumping Ogg page... (granule: {granule}, seek time: {})", self.guild_id, self.user_id, granule as f32 / sample_rate as f32);

            let page_header = OggHeader {
                continuation: false,
                begin_stream: false,
                end_stream: finalize,
                granule: granule as u64,
                serial: self.serial,
                sequence: state.sequence,
                segment_lengths: state.packet_buffer.lengths.clone(),
            };

            let page_data = page_header.build_page(state.packet_buffer.opus.as_slice()).unwrap();

            state.packet_buffer.clear();

            page_data
        };

        {
            let mut file =  self.file.lock().await;
            file.write_all(page_data.as_slice()).await.unwrap();
        }
    }

    pub async fn push(&self, rtp_update: RtpUpdate) {
        let opus_data = rtp_update.opus_data;
        let toc = OpusToc::from(*opus_data.first().unwrap());

        // trace!("[{}] <{}> RTP Timestamp: {}", self.guild_id, self.user_id, rtp_update.timestamp.0.0);

        let last_timestamp = self.state.lock().unwrap().packet_buffer.last_timestamp.clone();
        if let Some(last_timestamp) = last_timestamp {
            let timestamp_diff = rtp_update.timestamp - last_timestamp.0.0;
            if timestamp_diff.0.0 != toc.sample_count() as u32 {
                trace!("[{}] <{}> Dumping after {}s gap... [new = {}, last = {}, diff = {}, expected = {}]",
                    self.guild_id, self.user_id,
                    timestamp_diff.0.0 as f32 / toc.bandwidth.sample_rate() as f32,
                    rtp_update.timestamp.0.0,
                    last_timestamp.0.0,
                    timestamp_diff.0.0,
                    toc.sample_count()
                );

                self.dump(false).await;

                if timestamp_diff.0.0 > toc.sample_count() as u32 {
                    let silence_needed = timestamp_diff - toc.sample_count() as u32;
                    self.fill_silence(silence_needed.0.0 as usize, last_timestamp.0.0).await;
                }
            }
        }

        let dump = {
            let mut state = self.state.lock().unwrap();
            let packet_buffer = &mut state.packet_buffer;

            packet_buffer.lengths.push(opus_data.len() as u16);
            packet_buffer.total_length += opus_data.len();

            packet_buffer.tocs.push(toc);

            packet_buffer.opus.extend_from_slice(opus_data.as_slice());

            packet_buffer.last_timestamp = Some(rtp_update.timestamp);

            if packet_buffer.first_timestamp.is_none() {
                packet_buffer.first_timestamp = Some(rtp_update.timestamp);
            }

            packet_buffer.lengths.len() >= DUMP_PACKETS || packet_buffer.total_length >= DUMP_LENGTH
        };

        if dump {
            self.dump(false).await
        }
    }

    pub async fn finish(&self) {
        debug!("[{}] <{}> Finishing StreamWriter...", self.guild_id, self.user_id);
        self.dump(true).await
    }
}

impl Drop for StreamWriter {
    fn drop(&mut self) {
        trace!("[{}] <{}> StreamWriter::drop", self.guild_id, self.user_id);
    }
}