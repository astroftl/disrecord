use crate::recorder::writer::{RtpUpdate};
use dashmap::{DashMap};
use serenity::async_trait;
use serenity::model::id::UserId;
use serenity::model::voice_gateway::payload::Speaking;
use songbird::packet::{FromPacket};
use songbird::{EventContext, EventHandler};
use std::sync::Arc;
use serenity::all::GuildId;
use tokio::sync::mpsc::Sender;

#[derive(Clone, Debug)]
pub struct VoiceReceiver {
    pub inner: Arc<InnerReceiver>,
}

#[derive(Clone, Debug)]
pub struct InnerReceiver {
    guild_id: GuildId,
    rtp_tx: Sender<RtpUpdate>,
    ssrc_users: DashMap<u32, UserId>,
}

impl VoiceReceiver {
    pub async fn new(guild_id: GuildId, rtp_tx: Sender<RtpUpdate>) -> Self {
        Self {
            inner: Arc::new(InnerReceiver {
                guild_id,
                rtp_tx,
                ssrc_users: DashMap::new(),
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
            Ctx::SpeakingStateUpdate(Speaking { ssrc, user_id, .. }) => {
                if let Some(user) = user_id {
                    let user = UserId::from(user.0);
                    let old_ssrc = self.inner.ssrc_users.insert(*ssrc, user);
                    match old_ssrc {
                        None => {
                            debug!("[{}] Found new user {user} with SSRC {ssrc}", self.inner.guild_id);
                        }
                        Some(old_user) => {
                            if old_user == user {
                                debug!("[{}] SSRC {ssrc} was refreshed for user {user}", self.inner.guild_id);
                            } else {
                                warn!("[{}] SSRC {ssrc} reused! Was {old_user}, now {user}", self.inner.guild_id);
                            }
                        }
                    }
                }
            },
            Ctx::RtpPacket(rtp_data) => {
                let rtp = rtp_data.rtp().from_packet();
                let head = rtp_data.payload_offset;
                let tail = rtp_data.payload_end_pad;
                let (ssrc, timestamp, sequence) = (rtp.ssrc, rtp.timestamp, rtp.sequence);

                match self.inner.ssrc_users.get(&ssrc) {
                    None => {
                        error!("[{}] Got SSRC {ssrc} which does not match a known user!", self.inner.guild_id);
                    }
                    Some(user) => {
                        let opus_data = rtp.payload[head..rtp.payload.len() - tail].to_owned();

                        let rtp_update = RtpUpdate {
                            guild: self.inner.guild_id,
                            user: *user,
                            ssrc,
                            timestamp,
                            sequence,
                            opus_data,
                        };

                        self.inner.rtp_tx.send(rtp_update).await.unwrap();
                    }
                }
            },
            _ => {
                // We won't be registering this struct for any more event classes.
                unimplemented!()
            },
        }

        None
    }
}

impl Drop for VoiceReceiver {
    fn drop(&mut self) {
        trace!("VoiceReceiver::drop");
    }
}

impl Drop for InnerReceiver {
    fn drop(&mut self) {
        trace!("InnerReceiver::drop");
    }
}
