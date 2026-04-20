# Design Doc: Global Progress Tracing for Dictionary Importer

This document outlines the design for adding accurate, byte-level progress tracing to the `importer` library. The goal is to provide a simple, parseable output (1-100%) via the `tracing` crate, gated by a feature flag.

## 1. Requirements

- **Accuracy:** Progress must be based on total bytes processed across all dictionary bank files.
- **Performance:** Minimal overhead. Use atomic counters to avoid locking during parallel processing.
- **Simplicity:** Output should be simple strings like `"45%"` to be easily consumed by UI layers (e.g., GPUI/Zed).
- **Isolation:** Feature-gated under `progress` (which enables the `trace` feature).
- **Non-Invasive:** Avoid major API changes to the main import functions.

## 2. Architecture

### 2.1. Progress State
A shared internal state will track the global progress.

```rust
#[cfg(feature = "progress")]
struct ProgressContext {
    total_bytes: usize,
    processed_bytes: std::sync::atomic::AtomicUsize,
    last_reported_percentage: std::sync::atomic::AtomicUsize,
}
```

### 2.2. Data Flow
1.  **Initialization:** In `prepare_dictionary`, calculate the sum of sizes for all detected bank files (Term, Kanji, Tag, Meta).
2.  **Shared Reference:** Pass an `Option<Arc<ProgressContext>>` (or a similar lightweight reference) to the file processing functions.
3.  **Atomic Updates:** 
    - In `serde_json` loops: Update progress based on `stream.byte_offset()`.
    - In `simd_json` blocks: Update progress once the file is fully read/parsed.
4.  **Emission:** When the percentage increases, emit a `tracing::info!("{percentage}%")` event.

## 3. Implementation Details

### 3.1. Cargo.toml
Add the `progress` feature and link it to the existing `trace` feature.

```toml
[features]
progress = ["trace"]
trace = ["dep:tracing"]
```

### 3.2. Byte-Level Tracking
- For `serde_json` (streaming):
  ```rust
  let current_offset = stream.byte_offset();
  let delta = current_offset - last_offset;
  update_progress(delta);
  ```
- For `simd_json` (bulk):
  ```rust
  let file_size = outpath.metadata()?.len() as usize;
  update_progress(file_size);
  ```

### 3.3. Thread Safety
Since `rayon` is used for parallel imports, `ProgressContext` must be `Send + Sync`. Using `AtomicUsize` for both `processed_bytes` and `last_reported_percentage` ensures thread safety without mutexes.

## 4. Testing & Validation

- **Unit Test:** Verify that `ProgressContext` correctly calculates percentages and doesn't emit duplicate values.
- **Integration Test:** Run a dictionary import with the `progress` feature enabled and capture logs to ensure `1%` through `100%` are emitted in order.
- **Performance:** Benchmark a large import (e.g., 100MB+) with and without the `progress` feature to ensure negligible overhead.

## 5. Success Criteria

- Running `cargo run --features progress` emits percentage strings to stdout/logs.
- Progress reaches `100%` exactly when the last file is finished.
- No new public API parameters are added (the progress state is managed internally during the `prepare_dictionary` call).
