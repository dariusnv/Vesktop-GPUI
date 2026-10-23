# Performance measurement

No claim of being the fastest Discord client is established. Compare equal workloads
on the same machine, GPU, display scale, power mode and build configuration.

1. Build release mode with the committed lockfile. Record commit, OS, CPU/GPU/RAM,
   resolution, refresh rate, application mode and fixture/account.
2. Measure cold launch to first usable frame separately from warm launch. Use at
   least 20 runs for p50/p95. Process start time alone is not first-frame latency.
3. After startup settles, capture idle CPU, working set and private bytes for at least
   30 seconds, then repeat after ten minutes. The script reports CPU both as a
   fraction of one logical core and of the whole machine to avoid ambiguity.
4. Measure frame times while scrolling varied text, long paragraphs and attachments.
   Target p95 <= 16.7 ms at 60 Hz without sustained stalls above 50 ms. These are
   targets, not measured results. Confirm only visible rows plus overscan are laid out.
5. Repeat against upstream Vesktop with matching data. Include all Electron child
   processes in memory/CPU totals; the included script observes one process only.
   An offline native fixture and a fully connected Discord account are not equal workloads.

```powershell
# Run the native app first and leave it idle while measuring.
./scripts/measure.ps1 -Seconds 30 -OutputPath benchmark-idle.json
```

The implementation caps histories at 8 channels × 500 messages. This bounds message
counts, not all memory: strings, transient payloads, renderer/font/driver allocations,
catalogs and drafts also use memory. A future image cache must cap decoded bytes,
not just the number of image URLs. No custom allocator or speculative LTO tuning is used.

The UI currently materializes the small channel catalog while message rows are
virtualized. Stress testing large server catalogs is still required before treating
that as adequate for every account.

References: [GPUI list API](https://docs.rs/gpui/0.2.2/gpui/struct.ListState.html),
[Zed performance guidance](https://zed.dev/docs/development#performance-measurements),
[Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html).
