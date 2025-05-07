use std::cmp::min;
use std::path::PathBuf;
use std::sync::Mutex;
use rand::Rng;
use serenity::all::{GuildId, UserId};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex as AsyncMutex;
use crate::recorder::writer::muxer::ogg::{OggHeader, OggSegments, MAX_SEGMENTS_PER_FRAME};
use crate::recorder::writer::muxer::ogg_opus::{CommentHeader, IdHeader, MappingFamily, PRESKIP_DEFAULT};
use crate::recorder::writer::muxer::opus_toc::{Bandwidth, OpusToc};

const DISCORD_BANDWIDTH: Bandwidth = Bandwidth::Fullband;
const SILENCE_PACKET: [u8; 3] = [0xF8, 0xFF, 0xFE];
const SILENCE_TOC: OpusToc = OpusToc::from(SILENCE_PACKET[0]);
const MAX_SAMPLES_PER_PAGE: usize = 200_000;

#[derive(Debug)]
pub struct PacketBuffer {
    pub opus: Vec<u8>,
    pub tocs: Vec<OpusToc>,
    pub segments: OggSegments,
    pub total_samples: usize,
}

impl PacketBuffer {
    pub fn new() -> Self {
        Self {
            opus: Vec::new(),
            tocs: Vec::new(),
            segments: OggSegments::new(),
            total_samples: 0,
        }
    }

    pub fn clear(&mut self) {
        self.opus.clear();
        self.tocs.clear();
        self.segments.clear();
        self.total_samples = 0;
    }
}

#[derive(Debug)]
pub struct OpusState {
    sequence: u32,
    granule: u64,
    tick_count: usize,
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
    // TODO: Pass in a TOC so we can move away from assuming constant Discord bandwidths?
    pub async fn new(guild_id: GuildId, user_id: UserId, user_name: Option<String>, output_dir: PathBuf) -> Option<Self> {
        let rand_serial = rand::rng().random::<u32>();

        if let Err(e) = tokio::fs::create_dir_all(output_dir.clone()).await {
            error!("[{guild_id}] <{user_id}> Failed to create parent directory: {e:?}");
            return None;
        }

        let file_path = match user_name {
            None => output_dir.join(user_id.to_string() + ".opus"),
            Some(name) => output_dir.join(name + ".opus"),
        };

        trace!("[{guild_id}] <{user_id}> Creating output file: {}", file_path.display());

        match File::create(&file_path).await {
            Ok(file) => {
                let state = OpusState {
                    sequence: 0,
                    granule: 0,
                    tick_count: 0,
                    started: false,
                    packet_buffer: PacketBuffer::new(),
                };

                let stream = Self {
                    guild_id,
                    user_id,
                    serial: rand_serial,
                    state: Mutex::new(state),
                    file:  AsyncMutex::new(file),
                    file_path,
                };

                stream.start().await;

                Some(stream)
            }
            Err(e) => {
                error!("[{guild_id}] <{user_id}> Failed to create new file {}: {e:?}", file_path.display());
                None
            }
        }
    }

    pub async fn start(&self) {
        debug!("[{}] <{}> Starting file: {}", self.guild_id, self.user_id, self.file_path.display());

        let opus_id_header = IdHeader {
            channel_count: 2,
            preskip: PRESKIP_DEFAULT,
            input_sample_rate: DISCORD_BANDWIDTH.sample_rate(),
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
        };

        let mut id_page_segments = OggSegments::new();
        id_page_segments.push_packet(opus_id_data.len());

        trace!("[{}] <{}> ID page header: {id_page_header:?}", self.guild_id, self.user_id);

        let id_page = match id_page_header.build_page(&id_page_segments, opus_id_data.as_slice()) {
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
        };

        let mut comment_page_segments =  OggSegments::new();
        comment_page_segments.push_packet(opus_comment_data.len());

        trace!("[{}] <{}> Comment page header: {comment_page_header:?}", self.guild_id, self.user_id);

        let comment_page = match comment_page_header.build_page(&comment_page_segments, opus_comment_data.as_slice()) {
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
        }
    }

    pub async fn fill_silence(&self, ticks: usize) {
        let (mut sequence, mut last_granule) = {
            let state = self.state.lock().unwrap();
            (state.sequence, state.granule)
        };

        let mut packets_left = ticks;

        while packets_left > 0 {
            let page_packets = min(packets_left, MAX_SEGMENTS_PER_FRAME);

            let mut opus_data: Vec<u8> = Vec::new();
            let mut opus_segments = OggSegments::new();
            for _ in 0..page_packets {
                opus_data.extend_from_slice(&SILENCE_PACKET);
                opus_segments.push_packet(SILENCE_PACKET.len());
                last_granule += SILENCE_TOC.sample_count() as u64;
            }

            let page_header = OggHeader {
                continuation: false,
                begin_stream: false,
                end_stream: false,
                granule: last_granule as u64,
                serial: self.serial,
                sequence,
            };

            packets_left -= page_packets;
            sequence += 1;

            let page_data = page_header.build_page(&opus_segments, opus_data.as_slice()).unwrap();

            {
                let mut file =  self.file.lock().await;
                file.write_all(page_data.as_slice()).await.unwrap();
            }
        }
    }

    async fn dump(&self, finalize: bool) {
        let page_data = {
            let mut state = self.state.lock().unwrap();

            let granule = state.granule + state.packet_buffer.total_samples as u64;

            trace!("[{}] <{}> Dumping Ogg page... (granule: {granule})", self.guild_id, self.user_id);

            let page_header = OggHeader {
                continuation: false,
                begin_stream: false,
                end_stream: finalize,
                granule,
                serial: self.serial,
                sequence: state.sequence,
            };

            let page_data = page_header.build_page(&state.packet_buffer.segments, state.packet_buffer.opus.as_slice()).unwrap();

            state.packet_buffer.clear();
            state.granule = granule;
            state.sequence += 1;

            page_data
        };

        {
            let mut file =  self.file.lock().await;
            file.write_all(page_data.as_slice()).await.unwrap();
        }
    }

    pub async fn push_silence(&self, tick_count: usize) {
        self.push(&SILENCE_PACKET, tick_count).await;
    }

    pub async fn push(&self, opus_data: &[u8], tick_count: usize) {
        let toc = OpusToc::from(*opus_data.first().unwrap());

        let dump = {
            let state = &self.state.lock().unwrap();
            state.packet_buffer.segments.would_split(opus_data.len()).is_some() || state.packet_buffer.total_samples > MAX_SAMPLES_PER_PAGE
        };

        if dump {
            self.dump(false).await;
        }

        let mut state = self.state.lock().unwrap();

        if tick_count != state.tick_count + 1 {
            warn!("[{}] <{}> Discontinuous tick count! (was: {}, now: {tick_count})", self.guild_id, self.user_id, state.tick_count);
        }
        state.tick_count = tick_count;

        let packet_buffer = &mut state.packet_buffer;
        packet_buffer.segments.push_packet(opus_data.len());
        packet_buffer.total_samples += toc.sample_count();
        packet_buffer.tocs.push(toc);
        packet_buffer.opus.extend_from_slice(opus_data);
    }

    pub async fn finish(&self) {
        trace!("[{}] <{}> Finishing StreamWriter...", self.guild_id, self.user_id);
        self.dump(true).await
    }
}

impl Drop for StreamWriter {
    fn drop(&mut self) {
        trace!("[{}] <{}> StreamWriter::drop", self.guild_id, self.user_id);
    }
}