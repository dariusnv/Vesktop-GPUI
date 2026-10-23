//! Repeatable bounded-state workload; not a renderer or end-to-end benchmark.
use std::time::Instant;
use vesktop_core::{MAX_CACHED_CHANNELS, MAX_MESSAGES, MessageCache, demo_messages};

fn main() {
    let start = Instant::now();
    let mut cache = MessageCache::default();
    let channels = 1000u64;
    let messages_per_channel = 500usize;
    for channel_id in 1..=channels {
        for page in 0..5 {
            let mut messages = demo_messages(channel_id, 100);
            for message in &mut messages {
                message.id += page * 100;
            }
            cache.merge(channel_id, messages);
        }
    }
    assert_eq!(cache.len(), MAX_CACHED_CHANNELS * MAX_MESSAGES);
    println!(
        "{}",
        serde_json::json!({
            "channels_visited": channels,
            "messages_ingested": channels * messages_per_channel as u64,
            "retained_messages": cache.len(),
            "elapsed_ms": start.elapsed().as_secs_f64() * 1000.0,
            "scope": "Synthetic core-state workload only; excludes network, rendering and GPU."
        })
    );
}
