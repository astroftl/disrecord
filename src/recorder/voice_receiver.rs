use crate::recorder::writer::{OpusUpdate, UserUpdate, VoiceUpdate, VoiceUpdateType};
use dashmap::{DashMap};
use serenity::async_trait;
use serenity::model::id::UserId;
use serenity::model::voice_gateway::payload::Speaking;
use songbird::packet::FromPacket;
use songbird::{EventContext, EventHandler};
use std::sync::Arc;
use serenity::all::{Cache, CacheHttp, Context, GuildId, Http};
use songbird::packet::rtp::RtpExtensionPacket;
use tokio::sync::mpsc::Sender;

#[derive(Clone, Debug)]
struct CtxHolder {
    http: Arc<Http>,
    cache: Arc<Cache>,
}

impl CacheHttp for CtxHolder {
    fn http(&self) -> &Http {
        &self.http
    }

    fn cache(&self) -> Option<&Arc<Cache>> {
        Some(&self.cache)
    }
}

impl From<&Context> for CtxHolder {
    fn from(value: &Context) -> Self {
        Self {
            http: value.http.clone(),
            cache: value.cache.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct VoiceReceiver {
    pub inner: Arc<InnerReceiver>,
}

#[derive(Debug)]
pub struct InnerReceiver {
    guild_id: GuildId,
    voice_tx: Sender<VoiceUpdate>,
    ssrc_users: DashMap<u32, UserId>,
    ctx_holder: CtxHolder,
}

impl VoiceReceiver {
    pub async fn new(guild_id: GuildId, ctx: &Context, voice_tx: Sender<VoiceUpdate>) -> Self {
        Self {
            inner: Arc::new(InnerReceiver {
                guild_id,
                voice_tx,
                ssrc_users: DashMap::new(),
                ctx_holder: CtxHolder::from(ctx),
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
                    let username = match user.to_user(&self.inner.ctx_holder).await {
                        Ok(u) => {
                            Some(u.name)
                        }
                        Err(e) => {
                            warn!("[{}] <{user}> Failed to get username: {e:?}", self.inner.guild_id);
                            None
                        }
                    };

                    let old_ssrc = self.inner.ssrc_users.insert(*ssrc, user);
                    match old_ssrc {
                        None => {
                            debug!("[{}] Found new user {user} with SSRC {ssrc}", self.inner.guild_id);
                            let update_data = VoiceUpdate {
                                guild: self.inner.guild_id,
                                update: VoiceUpdateType::User(UserUpdate { user, username }),
                            };
                            self.inner.voice_tx.send(update_data).await.unwrap();
                        }
                        Some(old_user) => {
                            if old_user == user {
                                debug!("[{}] SSRC {ssrc} was refreshed for user {user}", self.inner.guild_id);
                            } else {
                                warn!("[{}] SSRC {ssrc} reused! Was {old_user}, now {user}", self.inner.guild_id);
                                let update_data = VoiceUpdate {
                                    guild: self.inner.guild_id,
                                    update: VoiceUpdateType::User(UserUpdate { user, username }),
                                };
                                self.inner.voice_tx.send(update_data).await.unwrap();
                            }
                        }
                    }
                }
            },
            Ctx::VoiceTick(voice_data) => {
                let mut update_data = Vec::new();

                for (ssrc, voice) in &voice_data.speaking {
                    if let Some(rtp_data) = &voice.packet {
                        match self.inner.ssrc_users.get(&ssrc) {
                            None => {
                                error!("[{}] Got SSRC {ssrc} which does not match a known user!", self.inner.guild_id);
                            }
                            Some(user) => {
                                let rtp = rtp_data.rtp().from_packet();

                                // let head = rtp_data.payload_offset;
                                // let tail = rtp_data.payload_end_pad;
                                // let opus_data = rtp.payload[head..rtp.payload.len() - tail].to_owned();

                                // Hack until I can fix Songbird's underlying sizing issues.
                                let ext_pkt = RtpExtensionPacket::new(&rtp.payload).unwrap();
                                let ext = ext_pkt.from_packet();
                                let payload = ext.payload;

                                // 20 is constant with current Discord encryption scheme.
                                let opus_data = payload[..payload.len() - 20].to_owned();
                                // trace!("rtp: {:02x?}", rtp_data.rtp().packet());
                                // trace!("ext: {:02x?}", rtp.payload);
                                // trace!("pay: {:02x?}", payload);
                                // trace!("opus: {:02x?}", opus_data);

                                let opus_update = OpusUpdate {
                                    user: *user,
                                    opus_data,
                                };

                                update_data.push(opus_update);
                            }
                        }
                    }
                }

                let voice_update = VoiceUpdate {
                    guild: self.inner.guild_id,
                    update: VoiceUpdateType::Opus(update_data),
                };

                self.inner.voice_tx.send(voice_update).await.unwrap();
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
