use std::sync::Arc;
use std::time::Instant;
use dashmap::DashMap;
use serenity::async_trait;
use serenity::model::voice_gateway::payload::{ClientDisconnect, Speaking};
use serenity::model::id::{UserId, GuildId};
use songbird::{EventContext, EventHandler};
use tokio::sync::mpsc::{channel, Sender};
use tokio::sync::Mutex;
use crate::recorder::Recorder;

pub const SAMPLES_PER_PACKET: u32 = 960;

#[derive(Debug)]
pub enum VoiceState {
    Speaking(Vec<i16>),
    Silent,
}

#[derive(Debug)]
pub struct UserVoiceState {
    pub user_id: UserId,
    pub timestamp: Instant,
    pub voice_state: VoiceState,
}

#[derive(Debug)]
pub struct VoiceData {
    pub user_voice_states: Vec<UserVoiceState>,
}

pub enum VoiceCommand {
    Record,
    Finish,
}

#[derive(Clone)]
pub struct VoiceReceiver {
    pub inner: Arc<InnerReceiver>,
}

pub struct InnerReceiver {
    known_ssrcs: DashMap<u32, UserId>,
    guild_id: GuildId,
    recorder: Mutex<Recorder>,
    voice_data: Sender<VoiceData>,
    pub voice_commands: Sender<VoiceCommand>,
}

impl VoiceReceiver {
    pub async fn new(guild_id: GuildId) -> Self {
        let (cmd_tx, cmd_rx) = channel(32);
        let (voice_tx, voice_rx) = channel(50);

        let recorder = Recorder::new(guild_id, voice_rx);

        let receiver = Self {
            inner: Arc::new(InnerReceiver {
                known_ssrcs: DashMap::new(),
                guild_id,
                recorder: Mutex::new(recorder),
                voice_data: voice_tx,
                voice_commands: cmd_tx,
            }),
        };

        let receiver_clone = receiver.clone();

        tokio::spawn(async move {
            let mut rx = cmd_rx;
            while let Some(command) = rx.recv().await {
                receiver_clone.handle_command(command).await;
            }
        });
        
        receiver
    }
    
    async fn handle_command(&self, command: VoiceCommand) {
        match command {
            VoiceCommand::Record => {
                debug!("[{}] Sending START_RECORDING command", self.inner.guild_id);
                self.inner.recorder.lock().await.begin_recording().await;
            }
            VoiceCommand::Finish => {
                debug!("[{}] Sending STOP_RECORDING command", self.inner.guild_id);
                self.inner.recorder.lock().await.finish_recording().await;
            }
        }
    }
}

#[async_trait]
impl EventHandler for VoiceReceiver {
    #[allow(unused_variables)]
    async fn act(&self, ctx: &EventContext<'_>) -> Option<songbird::Event> {
        use EventContext as Ctx;
        match ctx {
            Ctx::SpeakingStateUpdate(Speaking { speaking, ssrc, user_id, .. }) => {
                if let Some(user) = user_id {
                    let user = UserId::from(user.0);
                    let old_ssrc = self.inner.known_ssrcs.insert(*ssrc, user);
                    if old_ssrc.is_none() {
                        debug!("[{}] Speaking state update: user {user_id:?} has SSRC {ssrc:?}, using {speaking:?}", self.inner.guild_id);
                    }
                }
            },
            Ctx::VoiceTick(tick) => {
                let speaking = tick.speaking.len();
                let total_participants = speaking + tick.silent.len();
                
                let timestamp = Instant::now();

                // println!("Voice tick ({speaking}/{total_participants} live):");

                let mut new_data = VoiceData { user_voice_states: vec![] };

                for (ssrc, data) in &tick.speaking {
                    if let Some(user_id) = self.inner.known_ssrcs.get(ssrc).as_deref().cloned() {
                        let user_id_str = format!("{user_id:?}");

                        // This field should *always* exist under DecodeMode::Decode.
                        let decoded_voice = data.decoded_voice.as_ref().unwrap();
                        // let voice_len = decoded_voice.len();
                        // let audio_str = format!(
                        //     "first samples from {}: {:?}",
                        //     voice_len,
                        //     &decoded_voice[..voice_len.min(5)]
                        // );

                        new_data.user_voice_states.push(UserVoiceState { timestamp, user_id, voice_state: VoiceState::Speaking(decoded_voice.to_owned()) });
                    } else {
                        warn!("[{}] Got a voice packet with an SSRC not mapped to a user ID: {ssrc}", self.inner.guild_id)
                    }
                }

                for ssrc in &tick.silent {
                    if let Some(user_id) = self.inner.known_ssrcs.get(ssrc).as_deref().cloned() {
                        new_data.user_voice_states.push(UserVoiceState { timestamp, user_id, voice_state: VoiceState::Silent});
                    }// else {
                    //     trace!("[{}] Got a silence packet with an SSRC not mapped to a user ID: {ssrc}", self.inner.guild_id)
                    // }
                }

                if let Err(e) = self.inner.voice_data.send(new_data).await {
                    error!("[{}] Failed to send VoiceData over channel: {e:?}", self.inner.guild_id);
                }
            },
            Ctx::ClientDisconnect(ClientDisconnect { user_id, .. }) => {
                // You can implement your own logic here to handle a user who has left the
                // voice channel e.g., finalise processing of statistics etc.
                // You will typically need to map the User ID to their SSRC; observed when
                // first speaking.

                info!("[{}] Client disconnected: user {user_id:?}", self.inner.guild_id);
            },
            _ => {
                // We won't be registering this struct for any more event classes.
                unimplemented!()
            },
        }

        None
    }
}