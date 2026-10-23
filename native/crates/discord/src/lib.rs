//! Supported Discord bot API adapter. No personal-account token handling.
use anyhow::{Context as _, Result};
use async_channel::{Receiver, Sender};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use twilight_gateway::{Event, EventTypeFlags, Intents, Shard, ShardId, StreamExt as _};
use twilight_http::Client;
use twilight_model::{channel::ChannelType, id::Id};
use vesktop_core::{Attachment, Channel, Guild, Message, validate_message};

pub enum Command {
    Guilds,
    Channels(u64),
    History {
        request_id: u64,
        channel_id: u64,
        before: Option<u64>,
    },
    Send {
        channel_id: u64,
        content: String,
    },
}

pub enum Update {
    Connected(String),
    Guilds(Vec<Guild>),
    Channels {
        guild_id: u64,
        channels: Vec<Channel>,
    },
    History {
        request_id: u64,
        channel_id: u64,
        messages: Vec<Message>,
        older: bool,
    },
    Message(Message),
    Sent {
        message: Message,
        content: String,
    },
    Deleted {
        channel_id: u64,
        message_id: u64,
    },
    DeletedBulk {
        channel_id: u64,
        message_ids: Vec<u64>,
    },
    Disconnected(String),
    Failed {
        scope: Scope,
        operation: &'static str,
        message: String,
    },
    Status(String),
}

#[derive(Clone, Copy)]
pub enum Scope {
    Global,
    Guild(u64),
    History(u64),
    Send(u64),
}

pub struct Bridge {
    pub commands: Sender<Command>,
    pub updates: Receiver<Update>,
    active_channel: Arc<AtomicU64>,
    lagged: Arc<AtomicBool>,
}

impl Bridge {
    pub fn select_channel(&self, id: u64) {
        self.active_channel.store(id, Ordering::Relaxed);
    }
    pub fn take_lagged(&self) -> bool {
        self.lagged.swap(false, Ordering::Relaxed)
    }
}

pub fn start_bot(token: String) -> Bridge {
    let (commands, requests) = async_channel::bounded(16);
    let (events, updates) = async_channel::bounded(256);
    let active_channel = Arc::new(AtomicU64::new(0));
    let lagged = Arc::new(AtomicBool::new(false));
    let selected = active_channel.clone();
    let missed = lagged.clone();
    std::thread::Builder::new().name("discord-runtime".into()).spawn(move || {
        let runtime = match tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build() {
            Ok(runtime) => runtime,
            Err(_) => { let _ = events.send_blocking(Update::Failed { scope: Scope::Global, operation: "startup", message: "Could not start the network runtime.".into() }); return; }
        };
        runtime.block_on(async move {
            let http = Client::new(token.clone());
            let gateway_events = events.clone();
            let gateway = tokio::spawn(async move {
                let intents = Intents::GUILDS | Intents::GUILD_MESSAGES | Intents::MESSAGE_CONTENT;
                let mut shard = Shard::new(ShardId::ONE, token, intents);
                while let Some(event) = shard.next_event(EventTypeFlags::all()).await {
                    let update = match event {
                        Ok(Event::Ready(ready)) => Some(Update::Connected(ready.user.name)),
                        Ok(Event::GatewayClose(frame)) => Some(Update::Status(format!("Gateway closed ({}).", frame.map(|frame| frame.code).unwrap_or_default()))),
                        Ok(Event::MessageDeleteBulk(payload)) => Some(Update::DeletedBulk { channel_id: payload.channel_id.get(), message_ids: payload.ids.into_iter().map(|id| id.get()).collect() }),
                        Ok(Event::Resumed) => Some(Update::Status("Gateway reconnected.".into())),
                        Ok(Event::MessageCreate(payload)) => Some(Update::Message(map_message((*payload).0))),
                        Ok(Event::MessageUpdate(payload)) => Some(Update::Message(map_message((*payload).0))),
                        Ok(Event::MessageDelete(payload)) => Some(Update::Deleted { channel_id: payload.channel_id.get(), message_id: payload.id.get() }),
                        Err(_) => Some(Update::Status("Gateway interrupted. Reconnecting; refresh history after reconnect.".into())),
                        _ => None,
                    };
                    let Some(update) = update else { continue };
                    let relevant = match &update {
                        Update::Message(message) => message.channel_id == selected.load(Ordering::Relaxed),
                        Update::Deleted { channel_id, .. } | Update::DeletedBulk { channel_id, .. } => *channel_id == selected.load(Ordering::Relaxed),
                        _ => true,
                    };
                    // Never let a stalled renderer stop Gateway heartbeats.
                    if relevant && gateway_events.try_send(update).is_err() { missed.store(true, Ordering::Relaxed); }
                }
                let _ = gateway_events.send(Update::Disconnected("Gateway stopped. Check bot token and privileged intents, then restart to reconnect.".into())).await;
            });
            while let Ok(command) = requests.recv().await {
                let scope = match &command { Command::Guilds => Scope::Global, Command::Channels(id) => Scope::Guild(*id), Command::History { request_id, .. } => Scope::History(*request_id), Command::Send { channel_id, .. } => Scope::Send(*channel_id) };
                let operation = match &command {
                    Command::Guilds => "servers", Command::Channels(_) => "channels",
                    Command::History { .. } => "history", Command::Send { .. } => "send",
                };
                let result = tokio::time::timeout(std::time::Duration::from_secs(30), execute(&http, command)).await;
                let update = match result {
                    Ok(Ok(update)) => update,
                    // Don't forward raw request errors: they may include request internals.
                    Ok(Err(_)) => Update::Failed { scope, operation, message: "Discord request failed. Check bot permissions, token, Message Content intent and network access.".into() },
                    Err(_) => Update::Failed { scope, operation, message: "Discord request timed out. Refresh to retry. A timed-out send may have arrived; check history before resending.".into() },
                };
                if events.send(update).await.is_err() { break; }
            }
            gateway.abort();
        });
    }).expect("could not create network thread");
    Bridge {
        commands,
        updates,
        active_channel,
        lagged,
    }
}

async fn execute(http: &Client, command: Command) -> Result<Update> {
    Ok(match command {
        Command::Guilds => {
            // Paginate beyond 200 instead of silently dropping installed servers.
            let mut guilds = Vec::new();
            let mut after = None;
            loop {
                let page = match after {
                    Some(id) => {
                        http.current_user_guilds()
                            .after(id)
                            .limit(200)
                            .await?
                            .models()
                            .await?
                    }
                    None => {
                        http.current_user_guilds()
                            .limit(200)
                            .await?
                            .models()
                            .await?
                    }
                };
                let done = page.len() < 200;
                after = page.last().map(|guild| guild.id);
                guilds.extend(page.into_iter().map(|guild| Guild {
                    id: guild.id.get(),
                    name: guild.name,
                }));
                if done {
                    break;
                }
            }
            Update::Guilds(guilds)
        }
        Command::Channels(guild_id) => {
            let id = Id::new_checked(guild_id).context("invalid guild id")?;
            let channels = http
                .guild_channels(id)
                .await?
                .models()
                .await?
                .into_iter()
                .filter(|channel| {
                    matches!(
                        channel.kind,
                        ChannelType::GuildText | ChannelType::GuildAnnouncement
                    )
                })
                .map(|channel| Channel {
                    id: channel.id.get(),
                    guild_id,
                    name: channel.name.unwrap_or_default(),
                    topic: channel.topic.unwrap_or_default(),
                    position: channel.position.unwrap_or_default(),
                })
                .collect();
            Update::Channels { guild_id, channels }
        }
        Command::History {
            request_id,
            channel_id,
            before,
        } => {
            let id = Id::new_checked(channel_id).context("invalid channel id")?;
            let messages = match before {
                Some(before) => {
                    http.channel_messages(id)
                        .before(Id::new_checked(before).context("invalid message id")?)
                        .limit(100)
                        .await?
                        .models()
                        .await?
                }
                None => http.channel_messages(id).limit(100).await?.models().await?,
            };
            Update::History {
                request_id,
                channel_id,
                messages: messages.into_iter().map(map_message).collect(),
                older: before.is_some(),
            }
        }
        Command::Send {
            channel_id,
            content,
        } => {
            validate_message(&content).map_err(anyhow::Error::msg)?;
            let id = Id::new_checked(channel_id).context("invalid channel id")?;
            let no_mentions = twilight_model::channel::message::AllowedMentions::default();
            let message = http
                .create_message(id)
                .content(&content)
                .allowed_mentions(Some(&no_mentions))
                .await?
                .model()
                .await?;
            Update::Sent {
                message: map_message(message),
                content,
            }
        }
    })
}

fn map_message(message: twilight_model::channel::Message) -> Message {
    Message {
        id: message.id.get(),
        channel_id: message.channel_id.get(),
        author: message.author.global_name.unwrap_or(message.author.name),
        content: message.content,
        timestamp: message.timestamp.iso_8601().to_string(),
        attachments: message
            .attachments
            .into_iter()
            .map(|attachment| Attachment {
                name: attachment.filename,
                url: attachment.url,
            })
            .collect(),
    }
}
