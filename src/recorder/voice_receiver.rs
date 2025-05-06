use crate::recorder::voice_writer::VoiceWriter;
use dashmap::{DashMap, DashSet};
use serenity::async_trait;
use serenity::model::id::UserId;
use serenity::model::voice_gateway::payload::{ClientDisconnect, Speaking};
use songbird::packet::rtcp::report::ReportBlockPacket;
use songbird::packet::rtcp::RtcpPacket;
use songbird::packet::FromPacket;
use songbird::{EventContext, EventHandler};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::mpsc::{channel, Sender};
use tokio::sync::Mutex;
use crate::recorder::{RecordingMetadata, RecordingSummary, RtpUpdate, VoiceUpdate};

#[derive(Clone, Debug)]
pub struct VoiceReceiver {
    pub inner: Arc<InnerReceiver>,
}

#[derive(Clone, Debug)]
pub struct InnerReceiver {
    metadata: RecordingMetadata,
    update_tx: Sender<VoiceUpdate>,
    _writer: Arc<Mutex<VoiceWriter>>,
    ssrc_users: DashMap<u32, UserId>,
    known_users: DashSet<UserId>,
}

impl VoiceReceiver {
    pub async fn new(metadata: RecordingMetadata) -> Self {
        let (update_tx, update_rx) = channel(50);

        let writer = Arc::new(Mutex::new(VoiceWriter::new(metadata.clone())));
        VoiceWriter::run(writer.clone(), update_rx);

        Self {
            inner: Arc::new(InnerReceiver {
                metadata,
                update_tx,
                _writer: writer,
                ssrc_users: DashMap::new(),
                known_users: DashSet::new(),
            }),
        }
    }

    pub fn get_summary(&self) -> RecordingSummary {
        let known_users: HashSet<UserId> = self.inner.known_users.iter().map(|x| *x).collect();

        RecordingSummary {
            guild_id: self.inner.metadata.guild_id,
            output_dir: self.inner.metadata.output_dir.clone(),
            started: self.inner.metadata.started,
            known_users,
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
                            debug!("[{}] Found new user {user} with SSRC {ssrc}", self.inner.metadata.guild_id);
                            self.inner.known_users.insert(user);
                        }
                        Some(old_user) => {
                            if old_user == user {
                                debug!("[{}] SSRC {ssrc} was refreshed for user {user}", self.inner.metadata.guild_id);
                            } else {
                                warn!("[{}] SSRC {ssrc} reused! Was {old_user}, now {user}", self.inner.metadata.guild_id);
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
                // debug!("RTP: {rtp:?}");
                // debug!("RTP given start = {head}, given_tail: {tail}");
                // debug!("RTP ssrc: {ssrc}, timestamp: {timestamp:?}, sequence: {sequence:?}");
                // debug!("RTP packet: {:02x?}", rtp_data.rtp().packet());
                // debug!("RTP payload: {:02x?}", rtp_data.rtp().payload());

                match self.inner.ssrc_users.get(&ssrc) {
                    None => {
                        error!("[{}] Got SSRC {ssrc} which does not match a known user!", self.inner.metadata.guild_id);
                    }
                    Some(user) => {
                        let opus_data = rtp.payload[head..rtp.payload.len() - tail].to_owned();
                        self.inner.update_tx.send(VoiceUpdate::Rtp(RtpUpdate { user: *user, ssrc, timestamp, sequence, opus_data })).await.unwrap();
                    }
                }
            },
            Ctx::RtcpPacket(rtcp_data) => {
                let rtcp = rtcp_data.rtcp();
                match rtcp {
                    RtcpPacket::SenderReport(sr) => {
                        info!("Got RTCP SenderReport with {} blocks: {sr:?}", sr.get_rx_report_count());
                    }
                    RtcpPacket::ReceiverReport(rr) => {
                        // let trimmed_rr = ReceiverReportPacket::new(&rr[..])
                        let rr = rr.from_packet();
                        // debug!("Got RTCP ReceiverReport from {} with {} blocks!", rr.ssrc, rr.rx_report_count);
                        let payload = rr.payload;
                        let mut offset: usize = 0;
                        for x in 0..rr.rx_report_count {
                            let report = ReportBlockPacket::new(&payload[offset..offset + ReportBlockPacket::minimum_packet_size()]).unwrap().from_packet();
                            let ssrc = report.ssrc;
                            let ntp_middle = report.last_sr_timestamp;
                            let ntp_sec = (ntp_middle >> 16) & 0xFFFF;
                            let ntp_frac = ntp_middle & 0xFFFF;
                            offset += ReportBlockPacket::packet_size(&report);
                            // debug!("Got RR for SSRC {ssrc}, NTP {ntp_sec}.{ntp_frac} ({ntp_middle})");
                        }
                    }
                    RtcpPacket::KnownType(x) => {
                        warn!("Got RTCP of KnownType {x:?}");
                    }
                    _ => {
                        unreachable!("Got unknown RTCP packet type!");
                    }
                }
            },
            Ctx::ClientDisconnect(ClientDisconnect { user_id, .. }) => {
                info!("[{}] User {user_id} disconnected!", self.inner.metadata.guild_id);
                // TODO: Remove SSRC from known_users.
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
