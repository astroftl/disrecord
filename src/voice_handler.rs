use std::sync::Arc;
use std::time::Instant;
use dashmap::DashMap;
use serenity::async_trait;
use serenity::model::voice_gateway::payload::{ClientDisconnect, Speaking};
use serenity::model::id::{UserId, GuildId};
use songbird::{EventContext, EventHandler};
use tokio::sync::mpsc::{channel, Sender};
use tokio::sync::Mutex;
use crate::discord::RecordingMetadata;
use crate::recorder::Recorder;

#[derive(Debug)]
pub enum VoiceState {
    Speaking(Vec<i16>),
    Silent,
}

#[derive(Debug)]
pub struct UserVoiceState {
    pub user_id: UserId,
    pub voice_state: VoiceState,
}

#[derive(Debug)]
pub struct VoiceData {
    pub rx_timestamp: Instant,
    pub user_voice_states: Vec<UserVoiceState>,
}

#[derive(Clone)]
pub struct VoiceReceiver {
    pub inner: Arc<InnerReceiver>,
}

pub struct InnerReceiver {
    known_ssrcs: DashMap<u32, UserId>,
    guild_id: GuildId,
    _recorder: Arc<Mutex<Recorder>>,
    voice_tx: Sender<VoiceData>,
}

impl VoiceReceiver {
    pub async fn new(metadata: RecordingMetadata) -> Self {
        let (voice_tx, voice_rx) = channel(50);

        let guild_id = metadata.guild_id.clone();
        let recorder = Arc::new(Mutex::new(Recorder::new(metadata)));
        Recorder::run(recorder.clone(), voice_rx);

        Self {
            inner: Arc::new(InnerReceiver {
                known_ssrcs: DashMap::new(),
                guild_id,
                _recorder: recorder,
                voice_tx,
            }),
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
                        debug!("[{}] Found new user {} with SSRC {ssrc}, using {speaking:?}!", self.inner.guild_id, user);
                    }
                }
            },
            Ctx::VoiceTick(tick) => {
                let mut new_data = VoiceData { rx_timestamp: Instant::now(), user_voice_states: vec![] };

                for (ssrc, data) in &tick.speaking {
                    if let Some(user_id) = self.inner.known_ssrcs.get(ssrc).as_deref().cloned() {
                        // This field should *always* exist under DecodeMode::Decode.
                        let decoded_voice = data.decoded_voice.as_ref().unwrap();
                        new_data.user_voice_states.push(UserVoiceState { user_id, voice_state: VoiceState::Speaking(decoded_voice.to_owned()) });
                    } else {
                        warn!("[{}] Got a voice packet with an SSRC not mapped to a user ID: {ssrc}", self.inner.guild_id)
                    }
                }

                for ssrc in &tick.silent {
                    if let Some(user_id) = self.inner.known_ssrcs.get(ssrc).as_deref().cloned() {
                        new_data.user_voice_states.push(UserVoiceState { user_id, voice_state: VoiceState::Silent});
                    }
                }

                trace!("[{}] Sending VoiceData: {new_data:?}", self.inner.guild_id);

                if let Err(e) = self.inner.voice_tx.send(new_data).await {
                    error!("[{}] Failed to send VoiceData over channel: {e:?}", self.inner.guild_id);
                }
            },
            Ctx::ClientDisconnect(ClientDisconnect { user_id, .. }) => {
                info!("[{}] User {user_id} disconnected!", self.inner.guild_id);
            },
            _ => {
                // We won't be registering this struct for any more event classes.
                unimplemented!()
            },
        }

        None
    }
}