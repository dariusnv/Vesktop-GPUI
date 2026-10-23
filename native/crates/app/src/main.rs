#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
mod input;
use gpui::{prelude::*, *};
use input::{InputEvent, TextInput};
use std::collections::BTreeMap;
use vesktop_core::{
    Channel, Guild, Message, MessageCache, Snapshot, demo_catalog, demo_messages, validate_message,
};
use vesktop_discord::{Bridge, Command, Scope, Update};

const BG: u32 = 0x20232d;
const PANEL: u32 = 0x191c25;
const MUTED: u32 = 0x9199ad;
const TEXT: u32 = 0xe3e7f1;
const ACCENT: u32 = 0x9fafff;
actions!(native_client, [Quit, Find, Refresh, FocusComposer]);

struct ClientView {
    guilds: Vec<Guild>,
    channels: Vec<Channel>,
    guild_id: u64,
    channel_id: u64,
    cache: MessageCache,
    visible: Vec<Message>,
    list: ListState,
    list_key: (u64, String),
    next_request: u64,
    snapshots: BTreeMap<u64, Snapshot>,
    gateway_status: String,
    composer: Entity<TextInput>,
    search: Entity<TextInput>,
    _subscriptions: Vec<Subscription>,
    bridge: Option<Bridge>,
    drafts: BTreeMap<u64, String>,
    status: String,
    sending: Option<u64>,
    loading: bool,
    older_available: bool,
    show_details: bool,
    local_id: u64,
}

impl ClientView {
    fn new(bot_token: Option<String>, cx: &mut Context<Self>) -> Self {
        let composer = cx.new(|cx| TextInput::new("Write a message…", cx));
        let search = cx.new(|cx| TextInput::new("Search loaded messages · Ctrl+F", cx));
        let subscriptions = vec![
            cx.subscribe(&composer, |this, _, event, cx| {
                if matches!(event, InputEvent::Submitted) {
                    this.send(cx);
                }
            }),
            cx.subscribe(&search, |this, _, event, cx| {
                if matches!(event, InputEvent::Changed) {
                    this.rebuild(cx);
                }
            }),
        ];
        let bridge = bot_token.map(vesktop_discord::start_bot);
        let live = bridge.is_some();
        if let Some(bridge) = &bridge {
            let receiver = bridge.updates.clone();
            cx.spawn(async move |this, cx| {
                while let Ok(update) = receiver.recv().await {
                    if this.update(cx, |this, cx| this.apply(update, cx)).is_err() {
                        break;
                    }
                }
            })
            .detach();
            let _ = bridge.commands.try_send(Command::Guilds);
        }
        let (guilds, channels) = if live {
            (vec![], vec![])
        } else {
            demo_catalog()
        };
        let mut cache = MessageCache::default();
        if !live {
            cache.merge(11, demo_messages(11, 60));
        }
        let visible = cache
            .messages(11)
            .map(|items| items.iter().cloned().collect())
            .unwrap_or_default();
        Self {
            guilds,
            channels,
            guild_id: if live { 0 } else { 1 },
            channel_id: if live { 0 } else { 11 },
            list_key: (if live { 0 } else { 11 }, String::new()),
            next_request: 0,
            snapshots: BTreeMap::new(),
            gateway_status: if live { "Connecting" } else { "Offline" }.into(),
            cache,
            visible,
            list: ListState::new(if live { 0 } else { 60 }, ListAlignment::Bottom, px(300.)),
            composer,
            search,
            _subscriptions: subscriptions,
            bridge,
            drafts: BTreeMap::new(),
            status: if live {
                "Connecting to Discord as a bot…"
            } else {
                "Offline workspace · messages stay in memory until you close the app."
            }
            .into(),
            sending: None,
            loading: live,
            older_available: live,
            show_details: false,
            local_id: 1_000_000,
        }
    }

    fn request(&mut self, command: Command, cx: &mut Context<Self>) -> bool {
        if let Some(bridge) = &self.bridge {
            if bridge.commands.try_send(command).is_ok() {
                return true;
            }
            self.status = "Network request queue is busy. Try again shortly.".into();
            cx.notify();
        }
        false
    }

    fn save_draft(&mut self, cx: &App) {
        if self.channel_id != 0 {
            let content = self.composer.read(cx).content().to_owned();
            if content.is_empty() {
                self.drafts.remove(&self.channel_id);
            } else {
                self.drafts.insert(self.channel_id, content);
            }
        }
    }

    fn request_history(&mut self, before: Option<u64>, cx: &mut Context<Self>) {
        if self
            .snapshots
            .values()
            .any(|snapshot| snapshot.channel_id == self.channel_id)
        {
            self.loading = true;
            return;
        }
        self.next_request += 1;
        let request_id = self.next_request;
        if self.request(
            Command::History {
                request_id,
                channel_id: self.channel_id,
                before,
            },
            cx,
        ) {
            self.snapshots
                .insert(request_id, Snapshot::new(self.channel_id));
            self.loading = true;
        }
    }

    fn select_guild(&mut self, id: u64, cx: &mut Context<Self>) {
        if self.guild_id == id {
            return;
        }
        self.save_draft(cx);
        self.guild_id = id;
        self.channel_id = 0;
        self.composer
            .update(cx, |input, cx| input.set_content(String::new(), cx));
        if self.bridge.is_some() {
            self.channels.clear();
            self.loading = self.request(Command::Channels(id), cx);
        } else if let Some(channel) = self.channels.iter().find(|channel| channel.guild_id == id) {
            self.select_channel(channel.id, cx);
        }
        self.rebuild(cx);
    }

    fn select_channel(&mut self, id: u64, cx: &mut Context<Self>) {
        if id == self.channel_id {
            return;
        }
        self.save_draft(cx);
        self.channel_id = id;
        let draft = self.drafts.remove(&id).unwrap_or_default();
        self.composer
            .update(cx, |input, cx| input.set_content(draft, cx));
        self.cache.touch(id);
        self.older_available = self.bridge.is_some();
        if let Some(bridge) = &self.bridge {
            bridge.select_channel(id);
            self.request_history(None, cx);
        } else if self.cache.messages(id).is_none_or(|items| items.is_empty()) {
            self.cache.merge(id, demo_messages(id, 18));
        }
        self.rebuild(cx);
    }

    fn rebuild(&mut self, cx: &mut Context<Self>) {
        let query = self.search.read(cx).content().to_lowercase();
        let visible: Vec<Message> = self
            .cache
            .messages(self.channel_id)
            .into_iter()
            .flatten()
            .filter(|message| {
                query.is_empty()
                    || message.content.to_lowercase().contains(&query)
                    || message.author.to_lowercase().contains(&query)
            })
            .cloned()
            .collect();
        let key = (self.channel_id, query);
        if key != self.list_key {
            self.list.reset(visible.len());
            self.list_key = key;
        } else {
            let offset = self.list.logical_scroll_top();
            let anchor = self.visible.get(offset.item_ix).map(|message| message.id);
            let prefix = self
                .visible
                .iter()
                .zip(&visible)
                .take_while(|(left, right)| left == right)
                .count();
            let suffix = self.visible[prefix..]
                .iter()
                .rev()
                .zip(visible[prefix..].iter().rev())
                .take_while(|(left, right)| left == right)
                .count();
            if prefix + suffix < self.visible.len() || prefix + suffix < visible.len() {
                self.list.splice(
                    prefix..self.visible.len() - suffix,
                    visible.len() - prefix - suffix,
                );
            }
            if let Some(anchor) = anchor
                && let Some(index) = visible.iter().position(|message| message.id == anchor)
            {
                self.list.scroll_to(ListOffset {
                    item_ix: index,
                    offset_in_item: offset.offset_in_item,
                });
            }
        }
        self.visible = visible;
        cx.notify();
    }

    fn refresh(&mut self, _: &Refresh, _: &mut Window, cx: &mut Context<Self>) {
        self.refresh_data(cx);
    }
    fn refresh_data(&mut self, cx: &mut Context<Self>) {
        if self.bridge.is_none() {
            self.status = "Offline workspace. Connect with --bot to load Discord data.".into();
            cx.notify();
            return;
        }
        if self.channel_id == 0 {
            if self.guild_id == 0 {
                self.request(Command::Guilds, cx);
            } else {
                self.request(Command::Channels(self.guild_id), cx);
            }
        } else {
            self.request_history(None, cx);
        }
        cx.notify();
    }

    fn load_older(&mut self, cx: &mut Context<Self>) {
        if self.loading || !self.older_available {
            return;
        }
        let messages = self.cache.messages(self.channel_id);
        if messages.is_some_and(|messages| messages.len() >= vesktop_core::MAX_MESSAGES) {
            self.status =
                "The 500-message window is full. Earlier history paging is a future milestone."
                    .into();
            cx.notify();
            return;
        }
        let before = messages
            .and_then(|messages| messages.front())
            .map(|message| message.id);
        self.request_history(before, cx);
        cx.notify();
    }

    fn send(&mut self, cx: &mut Context<Self>) {
        if self.sending.is_some() || self.channel_id == 0 {
            return;
        }
        let content = self.composer.read(cx).content().to_owned();
        let content = match validate_message(&content) {
            Ok(content) => content.to_owned(),
            Err(error) => {
                self.status = error.into();
                cx.notify();
                return;
            }
        };
        if self.bridge.is_some() {
            if self.request(
                Command::Send {
                    channel_id: self.channel_id,
                    content,
                },
                cx,
            ) {
                self.sending = Some(self.channel_id);
                self.status = "Sending as the bot…".into();
            }
        } else {
            self.local_id += 1;
            self.cache.merge(
                self.channel_id,
                [Message {
                    id: self.local_id,
                    channel_id: self.channel_id,
                    author: "You · local".into(),
                    content,
                    timestamp: "just now".into(),
                    attachments: vec![],
                }],
            );
            self.composer.update(cx, |input, cx| input.clear(cx));
            self.status = "Saved in this offline session. Nothing was sent to Discord.".into();
            self.rebuild(cx);
        }
        cx.notify();
    }

    fn apply(&mut self, update: Update, cx: &mut Context<Self>) {
        match update {
            Update::Connected(name) => {
                self.gateway_status = "Connected".into();
                self.status = format!("Connected as bot {name}. Messages are sent as this bot.")
            }
            Update::Disconnected(status) => {
                self.gateway_status = "Disconnected".into();
                self.status = status;
            }
            Update::Status(status) => {
                self.gateway_status = if status.contains("reconnected") {
                    "Connected"
                } else {
                    "Reconnecting"
                }
                .into();
                self.status = status;
            }
            Update::Guilds(guilds) => {
                self.guilds = guilds;
                self.loading = false;
                if let Some(guild) = self.guilds.first() {
                    self.guild_id = guild.id;
                    self.loading = self.request(Command::Channels(guild.id), cx);
                } else {
                    self.status = "This bot is not installed in any servers.".into();
                }
            }
            Update::Channels {
                guild_id,
                mut channels,
            } => {
                if guild_id != self.guild_id {
                    return;
                }
                channels.sort_by_key(|channel| channel.position);
                self.channels = channels;
                self.loading = false;
                if let Some(channel) = self.channels.first() {
                    self.select_channel(channel.id, cx);
                } else {
                    self.status = "No text channels found in this server.".into();
                }
            }
            Update::History {
                request_id,
                channel_id,
                messages,
                older,
            } => {
                let Some(snapshot) = self.snapshots.remove(&request_id) else {
                    return;
                };
                let page_length = messages.len();
                let Some(messages) = snapshot.reconcile(messages) else {
                    if channel_id == self.channel_id {
                        self.loading = false;
                        self.status = "Too many live changes while loading. Refresh history to resynchronize.".into();
                        cx.notify();
                    }
                    return;
                };
                if channel_id == self.channel_id {
                    self.loading = false;
                    self.older_available = page_length == 100;
                    self.status = if messages.is_empty() {
                        "No more messages in this channel."
                    } else {
                        "History loaded."
                    }
                    .into();
                }
                if older {
                    self.cache.merge(channel_id, messages);
                } else {
                    self.cache.replace(channel_id, messages);
                }
                if channel_id == self.channel_id {
                    self.rebuild(cx);
                    if older && !self.visible.is_empty() {
                        self.list.scroll_to_reveal_item(0);
                    }
                }
            }
            Update::Message(message) => {
                for snapshot in self.snapshots.values_mut() {
                    snapshot.observe(&message);
                }
                let current = message.channel_id == self.channel_id;
                self.cache.merge(message.channel_id, [message]);
                if current {
                    self.rebuild(cx);
                }
            }
            Update::Sent { message, content } => {
                let id = message.channel_id;
                for snapshot in self.snapshots.values_mut() {
                    snapshot.observe(&message);
                }
                self.cache.merge(id, [message]);
                self.sending = None;
                if id == self.channel_id && self.composer.read(cx).content().trim() == content {
                    self.composer.update(cx, |input, cx| input.clear(cx));
                } else if self
                    .drafts
                    .get(&id)
                    .is_some_and(|draft| draft.trim() == content)
                {
                    self.drafts.remove(&id);
                }
                self.status = "Message sent as the bot.".into();
                if id == self.channel_id {
                    self.rebuild(cx);
                }
            }
            Update::Deleted {
                channel_id,
                message_id,
            } => {
                for snapshot in self.snapshots.values_mut() {
                    snapshot.delete(channel_id, message_id);
                }
                self.cache.delete(channel_id, message_id);
                if channel_id == self.channel_id {
                    self.rebuild(cx);
                }
            }
            Update::DeletedBulk {
                channel_id,
                message_ids,
            } => {
                for message_id in message_ids {
                    for snapshot in self.snapshots.values_mut() {
                        snapshot.delete(channel_id, message_id);
                    }
                    self.cache.delete(channel_id, message_id);
                }
                if channel_id == self.channel_id {
                    self.rebuild(cx);
                }
            }
            Update::Failed {
                scope,
                operation,
                message,
            } => {
                let current = match scope {
                    Scope::Global => true,
                    Scope::Guild(id) => id == self.guild_id,
                    Scope::History(id) => self
                        .snapshots
                        .remove(&id)
                        .is_some_and(|snapshot| snapshot.channel_id == self.channel_id),
                    Scope::Send(id) => {
                        if self.sending == Some(id) {
                            self.sending = None;
                        }
                        id == self.channel_id
                    }
                };
                if current {
                    self.loading = false;
                    self.status = format!("{operation}: {message}");
                }
            }
        }
        if self.bridge.as_ref().is_some_and(Bridge::take_lagged) {
            self.status =
                "Live updates exceeded the UI queue. Refresh to resynchronize history.".into();
        }
        cx.notify();
    }

    fn export(&mut self, cx: &mut Context<Self>) {
        let messages = self
            .cache
            .messages(self.channel_id)
            .map(|items| items.iter().collect::<Vec<_>>())
            .unwrap_or_default();
        match serde_json::to_string_pretty(&messages) {
            Ok(json) => {
                cx.write_to_clipboard(ClipboardItem::new_string(json));
                self.status = "Loaded history copied as JSON to your clipboard.".into();
            }
            Err(_) => self.status = "Could not export history.".into(),
        }
        cx.notify();
    }
    fn find(&mut self, _: &Find, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.search.focus_handle(cx));
    }
    fn focus_composer(&mut self, _: &FocusComposer, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.composer.focus_handle(cx));
    }

    fn message_row(message: &Message) -> Div {
        let initial = message
            .author
            .chars()
            .next()
            .unwrap_or('?')
            .to_uppercase()
            .to_string();
        div()
            .flex()
            .w_full()
            .gap_3()
            .px_6()
            .py_3()
            .hover(|style| style.bg(rgb(0x242835)))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .size_9()
                    .rounded_lg()
                    .bg(rgb(0x35405d))
                    .text_color(rgb(0xbfcbff))
                    .items_center()
                    .justify_center()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(initial),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .items_baseline()
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(TEXT))
                                    .child(message.author.clone()),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(MUTED))
                                    .child(message.timestamp.clone()),
                            ),
                    )
                    .child(div().text_color(rgb(0xcbd1df)).child(
                        if message.content.is_empty() && message.attachments.is_empty() {
                            "[No text content · check Message Content intent]".into()
                        } else {
                            message.content.clone()
                        },
                    ))
                    .children(message.attachments.iter().map(|attachment| {
                        let url = attachment.url.clone();
                        div()
                            .id(SharedString::from(format!(
                                "attachment-{}-{}",
                                message.id, attachment.name
                            )))
                            .mt_1()
                            .px_3()
                            .py_2()
                            .bg(rgb(0x2b3140))
                            .rounded_md()
                            .text_color(rgb(ACCENT))
                            .cursor_pointer()
                            .child(format!("↗  {}", attachment.name))
                            .on_click(move |_, _, cx| {
                                if url.starts_with("https://cdn.discordapp.com/")
                                    || url.starts_with("https://media.discordapp.net/")
                                {
                                    cx.open_url(&url);
                                }
                            })
                    })),
            )
    }
    fn button(id: &'static str, label: &'static str) -> Stateful<Div> {
        div()
            .id(id)
            .px_3()
            .py_2()
            .rounded_md()
            .text_xs()
            .text_color(rgb(0xbfc8df))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(0x343a4b)))
            .child(label)
    }
}

impl Render for ClientView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let live = self.bridge.is_some();
        let guild_name = self
            .guilds
            .iter()
            .find(|guild| guild.id == self.guild_id)
            .map(|guild| guild.name.clone())
            .unwrap_or_else(|| "Discord".into());
        let channel = self
            .channels
            .iter()
            .find(|channel| channel.id == self.channel_id);
        let channel_name = channel
            .map(|channel| channel.name.clone())
            .unwrap_or_else(|| "Select a channel".into());
        let topic = channel
            .map(|channel| channel.topic.clone())
            .unwrap_or_default();
        let channel_rows: Vec<_> = self
            .channels
            .iter()
            .filter(|channel| channel.guild_id == self.guild_id)
            .map(|channel| {
                let id = channel.id;
                div()
                    .id(SharedString::from(format!("channel-{id}")))
                    .h_9()
                    .mx_2()
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_3()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(rgb(if self.channel_id == id {
                        0x30374a
                    } else {
                        PANEL
                    }))
                    .text_color(rgb(if self.channel_id == id { TEXT } else { MUTED }))
                    .hover(|style| style.bg(rgb(0x282e3d)))
                    .child(div().text_lg().text_color(rgb(MUTED)).child("#"))
                    .child(channel.name.clone())
                    .on_click(cx.listener(move |this, _, _, cx| this.select_channel(id, cx)))
            })
            .collect();
        div().size_full().flex().flex_col().bg(rgb(BG)).text_color(rgb(TEXT)).font_family("Segoe UI").text_size(px(14.))
            .on_action(cx.listener(Self::find)).on_action(cx.listener(Self::refresh)).on_action(cx.listener(Self::focus_composer))
            .child(div().h_10().flex_none().px_5().flex().items_center().justify_between().bg(rgb(0x13161e)).border_b_1().border_color(rgb(0x2d3241))
                .child(div().flex().gap_3().items_center().child(div().font_weight(FontWeight::BOLD).child("VESKTOP"))
                    .child(div().text_xs().text_color(rgb(ACCENT)).child("NATIVE / GPUI")))
                .child(div().text_xs().text_color(rgb(if live { 0x90d7b3 } else { 0xd7c497 })).child(if live { format!("BOT SESSION · {}", self.gateway_status) } else { "OFFLINE WORKSPACE".to_owned() })))
            .child(div().flex().flex_1().min_h_0()
                .child(div().w_16().flex_none().py_4().flex().flex_col().items_center().gap_3().bg(rgb(0x13161e))
                    .children(self.guilds.iter().map(|guild| {
                        let id = guild.id;
                        let initials: String = guild.name.split_whitespace().filter_map(|part| part.chars().next()).take(2).collect();
                        div().id(SharedString::from(format!("guild-{id}"))).size_11().flex().items_center().justify_center().rounded_xl().cursor_pointer()
                            .bg(rgb(if id == self.guild_id { 0x6475c7 } else { 0x2a3040 })).font_weight(FontWeight::SEMIBOLD).child(initials)
                            .on_click(cx.listener(move |this, _, _, cx| this.select_guild(id, cx)))
                    })))
                .child(div().w(px(224.)).flex_none().flex().flex_col().bg(rgb(PANEL))
                    .child(div().h_16().px_5().flex().items_center().border_b_1().border_color(rgb(0x2b3040)).font_weight(FontWeight::SEMIBOLD).child(guild_name))
                    .child(div().px_5().pt_5().pb_3().text_xs().font_weight(FontWeight::SEMIBOLD).text_color(rgb(MUTED)).child("TEXT CHANNELS"))
                    .child(div().id("channels-scroll").flex_1().min_h_0().overflow_y_scroll().children(channel_rows))
                    .child(div().p_4().border_t_1().border_color(rgb(0x2b3040)).text_xs().text_color(rgb(MUTED))
                        .child(if live { "Public Discord API · bot account" } else { "Local sandbox · no account" })))
                .child(div().flex().flex_col().flex_1().min_w_0().min_h_0()
                    .child(div().h_16().flex_none().px_6().flex().items_center().gap_3().border_b_1().border_color(rgb(0x333849))
                        .child(div().text_xl().text_color(rgb(MUTED)).child("#"))
                        .child(div().font_weight(FontWeight::SEMIBOLD).child(channel_name))
                        .child(div().flex_1().min_w_0().text_xs().text_color(rgb(MUTED)).overflow_hidden().child(topic))
                        .child(Self::button("details", "About").on_click(cx.listener(|this, _, _, cx| { this.show_details = !this.show_details; cx.notify(); }))))
                    .child(div().flex_none().px_5().py_2().flex().items_center().gap_2().bg(rgb(0x232733))
                        .child(div().flex_1().min_w_0().child(self.search.clone()))
                        .child(Self::button("older", "Older").on_click(cx.listener(|this, _, _, cx| this.load_older(cx))))
                        .child(Self::button("refresh", "Refresh").on_click(cx.listener(|this, _, _, cx| this.refresh_data(cx))))
                        .child(Self::button("export", "Copy JSON").on_click(cx.listener(|this, _, _, cx| this.export(cx)))))
                    .when(self.show_details, |element| element.child(div().mx_5().mt_3().p_4().rounded_lg().bg(rgb(0x2e354a)).text_sm()
                        .child("Rust + GPUI prototype. Offline mode is an in-memory sandbox. Live mode uses a bot account. Personal login, DMs, rich Markdown, voice/video, screen sharing, plugins and persistent storage are not implemented. Ctrl+F: search · Ctrl+L: composer · Ctrl+R: refresh.")))
                    .child(div().flex_1().min_h_0().w_full().relative()
                        .when(self.visible.is_empty(), |element| element.child(div().size_full().flex().flex_col().items_center().justify_center().gap_3().text_color(rgb(MUTED))
                            .child(div().text_xl().text_color(rgb(TEXT)).child(if self.loading { "Loading…" } else { "A little room to talk." }))
                            .child("Select a channel, write a local message, or try another search.")))
                        .when(!self.visible.is_empty(), |element| element.child(list(self.list.clone(), cx.processor(|this, index, _, _| Self::message_row(&this.visible[index]).into_any_element())).size_full())))
                    .child(div().flex_none().px_5().pt_3().pb_2()
                        .child(div().flex().items_center().gap_3().p_3().rounded_lg().bg(rgb(0x292d39)).border_1().border_color(rgb(0x3c4357))
                            .child(div().flex_1().min_w_0().overflow_hidden().child(self.composer.clone()))
                            .child(Self::button("send", if self.sending.is_some() { "Sending…" } else { "Send ↵" }).bg(rgb(0x485889))
                                .on_click(cx.listener(|this, _, _, cx| this.send(cx)))))
                        .child(div().flex().justify_between().pt_2().text_xs().text_color(rgb(MUTED))
                            .child(if live { "Sends as your bot · mentions disabled" } else { "Enter to send locally · Ctrl+L to focus" })
                            .child(format!("{} loaded / {} cached", self.visible.len(), self.cache.len())))))
            )
            .child(div().flex_none().min_h_8().px_5().py_2().bg(rgb(0x171b24)).text_xs().text_color(rgb(MUTED)).child(self.status.clone()))
    }
}

fn main() {
    let bot = std::env::args().any(|argument| argument == "--bot");
    let bot_token = if bot {
        match std::env::var("VESKTOP_BOT_TOKEN")
            .ok()
            .filter(|token| !token.trim().is_empty())
        {
            Some(token) => Some(token.trim().trim_start_matches("Bot ").to_owned()),
            None => {
                eprintln!("--bot requires VESKTOP_BOT_TOKEN (a bot token, not a personal token).");
                std::process::exit(2);
            }
        }
    } else {
        None
    };
    Application::new().run(move |cx: &mut App| {
        input::bind_keys(cx);
        cx.bind_keys([
            KeyBinding::new("ctrl-q", Quit, None),
            KeyBinding::new("ctrl-f", Find, None),
            KeyBinding::new("ctrl-r", Refresh, None),
            KeyBinding::new("ctrl-l", FocusComposer, None),
        ]);
        cx.on_action(|_: &Quit, cx| cx.quit());
        let bounds = Bounds::centered(None, size(px(1240.), px(820.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(860.), px(560.))),
                titlebar: Some(TitlebarOptions {
                    title: Some("Vesktop Native · GPUI prototype".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_, cx| cx.new(|cx| ClientView::new(bot_token, cx)),
        )
        .expect("could not open GPUI window");
        cx.activate(true);
    });
}
