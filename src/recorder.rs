use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use circular_buffer::CircularBuffer;
use dashmap::DashMap;
use flacenc::error::{Verified, Verify};
use flacenc::source::{FrameBuf, MemSource, Source};
use serenity::all::{GuildId, UserId};
use tokio::sync::{mpsc, Mutex};
use crate::voice_handler::{VoiceData, VoiceState};
use flacenc::bitsink::ByteSink;
use tokio::fs::File as AsyncFile;
use tokio::io::AsyncWriteExt;
use flacenc::component::{BitRepr, Stream, StreamInfo};
use flacenc::{encode_fixed_size_frame};
use flacenc::config::Encoder;
use serenity::futures::executor::block_on;
use crate::discord::RecordingMetadata;

const BLOCK_SIZE: usize = 4096;
const BUFFER_N: usize = 2;
const BUFFER_FRAMES: usize = BUFFER_N * BLOCK_SIZE;

const SAMPLES_PER_PACKET: usize = 960;
const SILENT_SAMPLES: &[i16; SAMPLES_PER_PACKET] = &[0; SAMPLES_PER_PACKET];

const SAMPLE_RATE: usize = 48000;
const BITS_PER_SAMPLE: usize = 16;
const CHANNELS: usize = 1;

pub struct FlacEncoder {
    user_id: UserId,
    guild_id: GuildId,
    buffer: Mutex<CircularBuffer<BUFFER_FRAMES, i16>>,
    file: Mutex<AsyncFile>,
    channels: usize,
    bits_per_sample: usize,
    sample_rate: usize,
    enc_config: Verified<Encoder>,
    stream_info: StreamInfo,
}

impl FlacEncoder {
    pub async fn new(user_id: UserId, guild_id: GuildId, channels: usize, bits_per_sample: usize, sample_rate: usize, file_path: PathBuf) -> Option<Self> {
        trace!("[{guild_id}] <{user_id}> Initializing FlacEncoder...");
        let buffer = CircularBuffer::<BUFFER_FRAMES, i16>::new();
        
        // Default encoder settings with reasonable compression level
        let mut enc_config = Encoder::default();
        enc_config.block_size = BLOCK_SIZE;
        let enc_config = match enc_config.into_verified() {
            Ok(x) => x,
            Err(e) => {
                error!("[{guild_id}] <{user_id}> Failed to validate encoder config: {e:?}");
                return None;
            }
        };

        let mut stream_info = match StreamInfo::new(sample_rate, channels, bits_per_sample, ) {
            Ok(x) => x,
            Err(e) => {
                error!("[{guild_id}] <{user_id}> Failed to validate StreamInfo: {e:?}");
                return None;
            }
        };

        if let Err(e) = stream_info.set_block_sizes(enc_config.block_size, enc_config.block_size) {
            warn!("[{guild_id}] <{user_id}> Failed to set block sizes on FlacEncoder: {e:?}");
        }

        // Create the directory if it doesn't exist
        if let Some(parent) = file_path.parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                error!("[{guild_id}] <{user_id}> Failed to create parent directory: {e:?}");
                return None;
            }
        }

        // Open or create the file
        let file = match AsyncFile::create(&file_path).await {
            Ok(x) => x,
            Err(e) => {
                error!("[{guild_id}] <{user_id}> Failed to create new file {}: {e:?}", file_path.display());
                return None;
            }
        };

        info!("[{guild_id}] <{user_id}> Created new file: {}", file_path.display());
        
        Some(Self {
            user_id,
            guild_id,
            buffer: Mutex::new(buffer),
            file: Mutex::new(file),
            channels,
            bits_per_sample,
            sample_rate,
            enc_config,
            stream_info,
        })
    }
    
    // Initialize the FLAC file with appropriate headers
    pub async fn start(&self) {
        trace!("[{}] <{}> Starting FLAC file...", self.guild_id, self.user_id);

        let stream = Stream::with_stream_info(self.stream_info.clone());

        let mut header_sink = ByteSink::new();
        if let Err(e) = stream.write(&mut header_sink) {
            error!("[{}] <{}> Failed to write header to stream: {e:?}", self.guild_id, self.user_id);
        }

        let mut file = self.file.lock().await;
        if let Err(e) = file.write_all(header_sink.as_slice()).await {
            error!("[{}] <{}> Failed to write header to file: {e:?}", self.guild_id, self.user_id);
        }
    }
    
    // Encode a frame of audio data and append to the file
    async fn encode_frame(&self, samples: &Vec<i32>) {
        trace!("[{}] <{}> Encoding frame...", self.guild_id, self.user_id);

        let mut src = MemSource::from_samples(samples.as_slice(), self.channels, self.bits_per_sample, self.sample_rate);

        let mut frame_buf = match FrameBuf::with_size(src.channels(), self.enc_config.block_size) {
            Ok(x) => x,
            Err(e) => {
                error!("[{}] <{}> Failed to create new FrameBuf: {e:?}", self.guild_id, self.user_id);
                return;
            }
        };

        let read_samples = match src.read_samples(self.enc_config.block_size, &mut frame_buf) {
            Ok(x) => x,
            Err(e) => {
                error!("[{}] <{}> Failed to read samples from source: {e:?}", self.guild_id, self.user_id);
                return;
            }
        };

        if read_samples == 0 {
            error!("[{}] <{}> Called encode_frame on empty samples!", self.guild_id, self.user_id);
            return;
        }

        let frame = match encode_fixed_size_frame(&self.enc_config, &frame_buf, 0, &self.stream_info) {
            Ok(x) => x,
            Err(e) => {
                error!("[{}] <{}> Failed to encode frame: {e:?}", self.guild_id, self.user_id);
                return;
            }
        };

        let mut sink = ByteSink::new();
        if let Err(e) = frame.write(&mut sink) {
            error!("[{}] <{}> Failed to write bytes to sink: {e:?}", self.guild_id, self.user_id);
            return;
        }

        let mut file = self.file.lock().await;
        if let Err(e) = file.write_all(sink.as_slice()).await {
            error!("[{}] <{}> Failed to write frame to file: {e:?}", self.guild_id, self.user_id);
            return;
        }
    }

    pub async fn add_samples(&self, samples: &[i16]) {
        let mut buffer = self.buffer.lock().await;
        buffer.extend_from_slice(samples);

        trace!("[{}] <{}> Adding {} samples to the buffer...", self.guild_id, self.user_id, samples.len());

        if buffer.len() >= self.enc_config.block_size {
            trace!("[{}] <{}> Accumulated {} samples, writing block to encoder!", self.guild_id, self.user_id, buffer.len());

            let samples_to_encode = buffer.drain(..BLOCK_SIZE).map(|s| s as i32).collect::<Vec<_>>();
            drop(buffer); // Release the lock before async operation

            // Encode the frame
            self.encode_frame(&samples_to_encode).await;
        }
    }

    pub async fn add_silence(&self, duration: Duration) {
        if duration.is_zero() {
            return;
        }

        let duration_samples = duration.as_secs_f64() * SAMPLE_RATE as f64;
        let sample_count = duration_samples.trunc() as usize;
        let sample_quot = sample_count / BLOCK_SIZE;
        let sample_rem = sample_count % BLOCK_SIZE;

        debug!("[{}] <{}> Adding {}s worth of silence! [{sample_quot} blocks; {sample_rem} samples]", self.guild_id, self.user_id, duration.as_secs_f64());
        
        self.flush_buffer().await;

        let silence_block = vec![0; BLOCK_SIZE];
        for _ in 0..sample_quot {
            self.encode_frame(&silence_block).await;
        }

        let excess_silence = vec![0i16; sample_rem];
        self.add_samples(excess_silence.as_slice()).await;
    }
    
    async fn flush_buffer(&self) {
        let samples_to_encode = {
            let mut buffer = self.buffer.lock().await;
            let sample_count = buffer.len();
            buffer.drain(..sample_count).map(|s| s as i32).collect::<Vec<_>>()
        };

        if !samples_to_encode.is_empty() {
            debug!("[{}] <{}> Flushing {} remaining samples to encoder...", self.guild_id, self.user_id, samples_to_encode.len());
            self.encode_frame(&samples_to_encode).await;
        }
    }

    pub async fn finish(&self) {
        debug!("[{}] <{}> Dumping buffer and flushing file...", self.guild_id, self.user_id);
        
        self.flush_buffer().await;

        let mut file = self.file.lock().await;
        if let Err(e) = file.flush().await {
            error!("[{}] <{}> Failed to flush file to disk: {e:?}", self.guild_id, self.user_id);
            return;
        }
    }
}

impl Drop for FlacEncoder {
    fn drop(&mut self) {
        debug!("[{}] <{}> Finishing FlacEncoder before Drop...", self.guild_id, self.user_id);
        let f = self.finish();
        block_on(f);
        trace!("[{}] <{}> FlacEncoder dropped!", self.guild_id, self.user_id);
    }
}

pub struct Recorder {
    guild_id: GuildId,
    encoders: DashMap<UserId, Arc<FlacEncoder>>,
    output_dir: PathBuf,
    sample_rate: usize,
    channels: usize,
    bits_per_sample: usize,
    known_users: Arc<RwLock<HashSet<UserId>>>,
    started: Instant,
}

impl Recorder {
    pub fn new(metadata: RecordingMetadata) -> Self {
        Self {
            guild_id: metadata.guild_id,
            encoders: DashMap::new(),
            output_dir: metadata.output_dir,
            sample_rate: SAMPLE_RATE,
            channels: CHANNELS,
            bits_per_sample: BITS_PER_SAMPLE,
            known_users: metadata.known_users,
            started: Instant::now(),
        }
    }

    pub async fn get_or_create_encoder(&self, user_id: UserId, timestamp: Instant) -> Option<Arc<FlacEncoder>> {
        if let Some(encoder) = self.encoders.get(&user_id) {
            return Some(encoder.clone());
        }

        let output_path = self.output_dir.join(format!("{}.flac", user_id));
        let encoder = FlacEncoder::new(
            user_id,
            self.guild_id,
            self.channels,
            self.bits_per_sample,
            self.sample_rate,
            output_path,
        ).await;

        if let Some(enc) = encoder {
            enc.start().await;

            let silence_duration = timestamp.duration_since(self.started);
            enc.add_silence(silence_duration).await;

            let encoder = Arc::new(enc);

            self.encoders.insert(user_id, encoder.clone());

            Some(encoder)
        } else {
            error!("[{}] <{user_id}> Failed to create new FlacEncoder!", self.guild_id);
            None
        }
    }

    pub async fn add_audio_data(&self, user_id: UserId, timestamp: Instant, samples: &[i16]) {
        if let Some(encoder) = self.get_or_create_encoder(user_id, timestamp).await {
            encoder.add_samples(samples).await;
        } else {
            error!("[{}] <{user_id}> Failed to get FlacEncoder!", self.guild_id);
        }
    }

    pub async fn process_voice_data(&self, data: VoiceData) {
        let mut silent_this_packet = self.known_users.read().unwrap().clone();

        for voice_state in data.user_voice_states {
            if !silent_this_packet.contains(&voice_state.user_id) {
                debug!("[{}] <{}> User not previously in known user set, adding...", self.guild_id, voice_state.user_id);
                silent_this_packet.insert(voice_state.user_id);
                self.known_users.write().unwrap().insert(voice_state.user_id);
            }

            if let VoiceState::Speaking(samples) = voice_state.voice_state {
                silent_this_packet.remove(&voice_state.user_id);
                self.add_audio_data(voice_state.user_id, data.rx_timestamp, &samples).await;

                if samples.len() != SAMPLES_PER_PACKET {
                    warn!("[{}] <{}> We got a packet with a non-standard number of samples! Got: {}, expected: {SAMPLES_PER_PACKET})", self.guild_id, voice_state.user_id, samples.len());
                }
            }
        }

        for user in silent_this_packet.iter() {
            self.add_audio_data(*user, data.rx_timestamp, SILENT_SAMPLES).await;
        }
    }

    pub fn run(recorder: Arc<Mutex<Self>>, mut voice_rx: mpsc::Receiver<VoiceData>) {
        tokio::spawn(async move {
            while let Some(voice_data) = voice_rx.recv().await {
                recorder.lock().await.process_voice_data(voice_data).await;
            }
        });
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        trace!("[{}] Recorder is being Drop'd...", self.guild_id);
        self.encoders.clear();
        info!("[{}] Recorder Drop'd!", self.guild_id);
    }
}