# Validation record

Date: 2026-10-24 (Europe/Bucharest). Windows-first prototype milestone.

## Build and correctness

- `cargo fmt --all -- --check`: clean after formatting.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo test --workspace`: 5 tests passed; no failures.
- `cargo build --release --locked`: passed, including Windows HLSL compilation.
- `scripts/build.ps1`: produced the portable binary and accompanying licenses.
- `--bot` with no `VESKTOP_BOT_TOKEN`: exits with code 2 and an explanatory error.

The dependency tree currently produces a future-compatibility warning for upstream
`proc-macro-error2 2.0.1`. It does not prevent builds on the tested toolchain.

## Native window smoke check

The release executable opened a native GPUI window. A Windows UI check verified:

- Native shell/transcript layout, wrapping and visible sample data.
- Unicode input containing an emoji and an em dash.
- Enter added the local message, cleared the composer and updated the loaded count.
- Ctrl+F focused search; typing filtered the transcript to the matching message.
- Clearing the query restored the complete loaded transcript.

IME offset conversion has a regression test. Real CJK input methods, native
accessibility, screen readers, drag selection and large multi-server catalogs still
need broader interactive validation. Live API operations were compiled, not tested
against Discord: no authorized bot credentials were supplied.

## Initial observations

Machine: Windows 11 IoT Enterprise 10.0.26300; AMD Ryzen 7 7800X3D (16 logical
processors); NVIDIA GeForce RTX 5080. Rust 1.99.0, GPUI 0.2.2, release profile.

Thirty-second observation of the offline sample workload, after focusing the composer:

| Measurement | Observation |
| --- | ---: |
| Mean working set | 68.72 MiB |
| Peak working set | 68.81 MiB |
| Mean private bytes | 95.90 MiB |
| CPU as fraction of one logical core | 1.761% |
| CPU as fraction of whole 16-thread machine | 0.110% |
| Sample duration | 30.162 seconds |

The stripped Windows binary is about 20.1 MiB. These numbers do not establish
end-to-end superiority to Electron: no equal-workload Electron baseline, startup
distribution, frame-time trace, connected-account test or ten-minute idle test was
run. Do not compare an offline fixture with a connected full Discord session.

The release `cache_probe` example ingested 500,000 synthetic messages across 1,000
channels in approximately 131 ms and retained 4,000 messages (8 × 500). This tests
only core state and eviction; it excludes network, rendering, text layout and GPU.
Timing varies by machine and load. Run it with:

```powershell
cargo run --release -p vesktop-core --example cache_probe
```

CI is configured for Windows build/lint/tests, but remote workflow execution has
not been verified as part of this local validation record.
