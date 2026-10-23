//! Bounded, transport-independent chat state. No GUI or network dependencies.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

pub const MAX_CACHED_CHANNELS: usize = 8;
pub const MAX_MESSAGES: usize = 500;
pub const MAX_CONTENT_UTF16: usize = 2000;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Guild {
    pub id: u64,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Channel {
    pub id: u64,
    pub guild_id: u64,
    pub name: String,
    pub topic: String,
    pub position: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Attachment {
    pub name: String,
    pub url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Message {
    pub id: u64,
    pub channel_id: u64,
    pub author: String,
    pub content: String,
    pub timestamp: String,
    pub attachments: Vec<Attachment>,
}

/// LRU channel cache with bounded, chronological, ID-deduplicated histories.
#[derive(Default)]
pub struct MessageCache {
    histories: BTreeMap<u64, VecDeque<Message>>,
    recency: VecDeque<u64>,
}

impl MessageCache {
    pub fn replace(&mut self, channel_id: u64, messages: impl IntoIterator<Item = Message>) {
        self.touch(channel_id);
        self.histories.get_mut(&channel_id).unwrap().clear();
        self.merge(channel_id, messages);
    }
    pub fn messages(&self, channel_id: u64) -> Option<&VecDeque<Message>> {
        self.histories.get(&channel_id)
    }

    pub fn touch(&mut self, channel_id: u64) {
        self.recency.retain(|id| *id != channel_id);
        self.recency.push_back(channel_id);
        self.histories.entry(channel_id).or_default();
        while self.recency.len() > MAX_CACHED_CHANNELS {
            if let Some(id) = self.recency.pop_front() {
                self.histories.remove(&id);
            }
        }
    }

    /// Merge REST pages and Gateway events, which can arrive out of order.
    pub fn merge(&mut self, channel_id: u64, incoming: impl IntoIterator<Item = Message>) {
        self.touch(channel_id);
        let history = self.histories.get_mut(&channel_id).unwrap();
        let mut ordered: BTreeMap<u64, Message> = history.drain(..).map(|m| (m.id, m)).collect();
        for message in incoming {
            if message.channel_id == channel_id {
                ordered.insert(message.id, message);
            }
        }
        while ordered.len() > MAX_MESSAGES {
            ordered.pop_first();
        }
        history.extend(ordered.into_values());
    }

    pub fn delete(&mut self, channel_id: u64, message_id: u64) {
        if let Some(messages) = self.histories.get_mut(&channel_id) {
            messages.retain(|message| message.id != message_id);
        }
    }

    pub fn clear(&mut self) {
        self.histories.clear();
        self.recency.clear();
    }

    pub fn len(&self) -> usize {
        self.histories.values().map(VecDeque::len).sum()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Replays live changes that arrive while a REST snapshot is in flight.
/// The journal is bounded; an overflow discards the snapshot and asks for refresh.
pub struct Snapshot {
    pub channel_id: u64,
    changes: BTreeMap<u64, Option<Message>>,
    overflow: bool,
}

impl Snapshot {
    pub fn new(channel_id: u64) -> Self {
        Self {
            channel_id,
            changes: BTreeMap::new(),
            overflow: false,
        }
    }
    pub fn observe(&mut self, message: &Message) {
        if message.channel_id == self.channel_id {
            self.record(message.id, Some(message.clone()));
        }
    }
    pub fn delete(&mut self, channel_id: u64, message_id: u64) {
        if channel_id == self.channel_id {
            self.record(message_id, None);
        }
    }
    fn record(&mut self, id: u64, message: Option<Message>) {
        if self.changes.len() >= MAX_MESSAGES && !self.changes.contains_key(&id) {
            self.overflow = true;
            return;
        }
        self.changes.insert(id, message);
    }
    pub fn reconcile(self, messages: Vec<Message>) -> Option<Vec<Message>> {
        if self.overflow {
            return None;
        }
        let mut messages: BTreeMap<_, _> = messages
            .into_iter()
            .filter(|message| message.channel_id == self.channel_id)
            .map(|message| (message.id, message))
            .collect();
        for (id, change) in self.changes {
            match change {
                Some(message) => {
                    messages.insert(id, message);
                }
                None => {
                    messages.remove(&id);
                }
            }
        }
        Some(messages.into_values().collect())
    }
}

pub fn validate_message(content: &str) -> Result<&str, &'static str> {
    let content = content.trim();
    if content.is_empty() {
        return Err("Write a message first.");
    }
    if content.encode_utf16().count() > MAX_CONTENT_UTF16 {
        return Err("Messages are limited to 2,000 characters (UTF-16 units).");
    }
    Ok(content)
}

pub fn demo_catalog() -> (Vec<Guild>, Vec<Channel>) {
    let guilds = vec![
        Guild {
            id: 1,
            name: "Native Lab".into(),
        },
        Guild {
            id: 2,
            name: "GPUI Workshop".into(),
        },
    ];
    let channels = [
        (
            11,
            1,
            "general",
            "A native client, built one working slice at a time.",
        ),
        (
            12,
            1,
            "development",
            "Architecture, implementation and useful discoveries.",
        ),
        (
            13,
            1,
            "performance",
            "Measure first. Keep the hot path small.",
        ),
        (21, 2, "welcome", "Explore the native interface."),
        (22, 2, "ui-experiments", "A place to try things."),
    ]
    .into_iter()
    .enumerate()
    .map(|(position, (id, guild_id, name, topic))| Channel {
        id,
        guild_id,
        name: name.into(),
        topic: topic.into(),
        position: position as i32,
    })
    .collect();
    (guilds, channels)
}

pub fn demo_messages(channel_id: u64, count: usize) -> Vec<Message> {
    let samples = [
        (
            "Mira",
            "Welcome to Native Lab. This is an offline workspace: nothing here is sent to Discord.",
        ),
        (
            "Alex",
            "The renderer is GPUI. There is no browser, DOM, JavaScript runtime or webview in this executable.",
        ),
        (
            "Sam",
            "Try switching channels, writing a message, and searching the loaded history with Ctrl+F.",
        ),
        (
            "Mira",
            "The transcript uses variable-height virtualization. Long messages should wrap naturally, and offscreen rows stay out of the element tree. 🦀",
        ),
        (
            "Alex",
            "Performance claims need measurements. A different toolkit alone does not make an app the fastest client.",
        ),
        (
            "Sam",
            "Live developer mode connects a bot through the public Discord API. Personal-account login, voice, video and Vencord plugins are still future work.",
        ),
    ];
    (0..count)
        .map(|index| {
            let (author, content) = samples[index % samples.len()];
            Message {
                id: index as u64 + 1,
                channel_id,
                author: author.into(),
                content: content.into(),
                timestamp: format!("10:{:02}", index % 60),
                attachments: vec![],
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rest_gateway_overlap_stays_sorted_and_bounded() {
        let mut cache = MessageCache::default();
        let mut messages = demo_messages(11, 700);
        messages.reverse();
        cache.merge(11, messages);
        cache.merge(11, demo_messages(11, 500));
        let history = cache.messages(11).unwrap();
        assert_eq!(history.len(), MAX_MESSAGES);
        assert_eq!(history.front().unwrap().id, 201);
        assert_eq!(history.back().unwrap().id, 700);
    }

    #[test]
    fn lru_preserves_recent_channel_and_rejects_wrong_channel() {
        let mut cache = MessageCache::default();
        for id in 1..=8 {
            cache.merge(id, demo_messages(id, 3));
        }
        cache.touch(1);
        cache.merge(9, demo_messages(42, 3));
        assert!(cache.messages(1).is_some());
        assert!(cache.messages(2).is_none());
        assert!(cache.messages(9).unwrap().is_empty());
        cache.delete(1, 2);
        assert_eq!(cache.messages(1).unwrap().len(), 2);
    }

    #[test]
    fn content_limit_counts_discord_utf16_units() {
        assert!(validate_message(" \n ").is_err());
        assert!(validate_message(&"🦀".repeat(1000)).is_ok());
        assert!(validate_message(&"🦀".repeat(1001)).is_err());
        assert_eq!(validate_message(" hello "), Ok("hello"));
    }

    #[test]
    fn snapshot_does_not_resurrect_deletions_or_undo_live_edits() {
        let old = demo_messages(11, 3);
        let mut snapshot = Snapshot::new(11);
        snapshot.delete(11, 1);
        let mut edited = old[1].clone();
        edited.content = "edited after request began".into();
        snapshot.observe(&edited);
        let reconciled = snapshot.reconcile(old).unwrap();
        assert_eq!(reconciled.len(), 2);
        assert_eq!(reconciled[0].content, edited.content);
        let mut cache = MessageCache::default();
        cache.merge(11, demo_messages(11, 3));
        cache.replace(11, reconciled);
        assert_eq!(cache.messages(11).unwrap().len(), 2);
        cache.replace(11, []);
        assert!(cache.messages(11).unwrap().is_empty());
    }
}
