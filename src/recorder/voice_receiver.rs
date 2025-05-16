use std::fmt::Debug;
use crate::recorder::writer::{OpusUpdate, UserUpdate, VoiceUpdate, VoiceUpdateType};
use dashmap::{DashMap};
use serenity::async_trait;
use serenity::model::id::UserId;
use serenity::model::voice_gateway::payload::Speaking;
use songbird::packet::FromPacket;
use songbird::{Call, EventContext, EventHandler};
use std::sync::Arc;
use serenity::all::{Cache, CacheHttp, ChannelId, Context, GuildId, Http};
use songbird::packet::rtp::RtpExtensionPacket;
use tokio::sync::mpsc::Sender;
use tokio::sync::Mutex;
use crate::recorder::recorder::Recorder;
use crate::recorder::writer::opus_toc::{FrameCount, FrameSize, OpusToc};

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
    ctx: Context,
    call: Arc<Mutex<Call>>,
}

impl VoiceReceiver {
    pub async fn new(guild_id: GuildId, ctx: &Context, call: Arc<Mutex<Call>>, voice_tx: Sender<VoiceUpdate>) -> Self {
        Self {
            inner: Arc::new(InnerReceiver {
                guild_id,
                voice_tx,
                ssrc_users: DashMap::new(),
                ctx: ctx.clone(),
                call,
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
                    let username = match user.to_user(&self.inner.ctx).await {
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

                                if rtp.padding == 1 {
                                    trace!("[{}] <{}> Skipping RTP padding packet: {rtp:?}\n{:02x?}", self.inner.guild_id, user.value(), rtp.payload);
                                    continue;
                                }

                                // let head = rtp_data.payload_offset;
                                // let tail = rtp_data.payload_end_pad;
                                // let opus_data = rtp.payload[head..rtp.payload.len() - tail].to_owned();

                                // Hack until I can fix Songbird's underlying sizing issues.
                                let payload = if rtp.extension == 1 {
                                    let ext_pkt = RtpExtensionPacket::new(&rtp.payload).unwrap();
                                    let ext = ext_pkt.from_packet();
                                    ext.payload
                                } else {
                                    trace!("[{}] <{}> Got RTP without extension: {rtp:?}\n{:02x?}", self.inner.guild_id, user.value(), rtp.payload);
                                    rtp.payload
                                };

                                // 20 is constant with current Discord encryption scheme.
                                let opus_data = if payload.len() > 20 {
                                    payload[..payload.len() - 20].to_owned()
                                } else {
                                    warn!("[{}] <{}> Got payload less than 20: {payload:02x?}", self.inner.guild_id, user.value());
                                    payload.to_owned()
                                };

                                // trace!("rtp: {:02x?}", rtp_data.rtp().packet());
                                // trace!("ext: {:02x?}", rtp.payload);
                                // trace!("pay: {:02x?}", payload);
                                // trace!("opus: {:02x?}", opus_data);

                                let toc = OpusToc::from(opus_data[0]);
                                
                                let mut show_packet = false;
                                if toc.frame_count != FrameCount::One {
                                    debug!("[{}] <{}> Got an abnormal frame count: {:?}", self.inner.guild_id, user.value(), toc.frame_count);
                                    show_packet = true;
                                }

                                if toc.frame_size != FrameSize::Ms20 {
                                    debug!("[{}] <{}> Got an abnormal frame size: {:?}", self.inner.guild_id, user.value(), toc.frame_size);
                                    show_packet = true;
                                }

                                if toc.stereo == true {
                                    debug!("[{}] <{}> Got a stereo packet!", self.inner.guild_id, user.value());
                                    show_packet = true;
                                }

                                if show_packet {
                                    debug!("[{}] <{}> Abnormal TOC [{:02x}]: {toc:?}", self.inner.guild_id, user.value(), opus_data.first().unwrap());
                                }

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
                    update: VoiceUpdateType::VcUpdate(update_data),
                };

                self.inner.voice_tx.send(voice_update).await.unwrap();
            },
            Ctx::ClientDisconnect(disconnect_data) => {
                // TODO: Check if we're the last user in the channel, then disconnect.
                let channel_opt = self.inner.call.lock().await.current_channel();
                if let Some(channel_sid) = channel_opt {
                    match self.inner.guild_id.channels(&self.inner.ctx.http).await {
                        Ok(channels) => {
                            let channel_id = ChannelId::from(channel_sid.0);
                            let channel_opt = channels.get(&channel_id);
                            if let Some(channel) = channel_opt {
                                let members = channel.members(&self.inner.ctx.cache);
                                match members {
                                    Ok(members) => {
                                        debug!("[{}] New member list: {members:?}", self.inner.guild_id);
                                        if members.len() == 1 {
                                            let own_id = self.inner.ctx.cache.current_user().id;
                                            if members.first().unwrap().user.id == own_id {
                                                info!("[{}] All members have left the channel, ending the recording!", self.inner.guild_id);
                                                let recorder = Recorder::get(&self.inner.ctx).await.unwrap();
                                                recorder.finish_self(&self.inner.ctx, self.inner.guild_id).await;
                                            } else {
                                                warn!("[{}] Voice channel only has one person, and it's not us!", self.inner.guild_id);
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        error!("[{}] Failed to get members from cache: {e:?}", self.inner.guild_id);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!("[{}] Failed to get channels from guild for disconnect check: {e:?}", self.inner.guild_id);
                        }
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
