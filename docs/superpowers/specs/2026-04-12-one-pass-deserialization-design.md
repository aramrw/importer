# Design Doc: One-Pass Serde Deserialization for Yomichan Importer

Optimizing JSON deserialization by replacing all double-pass `serde_json::Value` buffering and `#[serde(untagged)]` enums with explicit, one-pass `Visitor` implementations.

## 1. Problem Statement
The current importer takes ~12 seconds for a large dictionary (98 term bank files).
The flamegraph shows:
- **44.4%** of time in `serde::de::Deserializer::__deserialize_content`.
- **13.1%** of time in `std::io::uninlined_slow_read_byte`.
- Significant overhead from `#[serde(untagged)]` trial-and-error parsing in `Element` and `MetaDataMatchType`.
- Double-parsing in `ContentMatchType` and `MetaDataMatchType` due to intermediate `serde_json::Value` buffering.

The goal is to reach **sub-5 seconds** by eliminating these overheads and ensuring parallel processing.

## 2. Proposed Changes

### 2.1. `ContentMatchType` One-Pass Visitor
Replace the `serde_json::Value` buffering in `ContentMatchType::visit_map`.

- **Strategy**: 
    1. Use `map.next_key::<CompactString>()` to identify the first field.
    2. If the key is `"type"`, it is a `StructuredContent` wrapper (e.g., `{"type":"structured-content", "content": [...]}`).
    3. If the key is `"tag"`, it is an `Element` object (e.g., `{"tag":"div", "content": [...]}`).
    4. Deserialize the remaining fields directly into the corresponding struct without intermediate buffering.

### 2.2. `Element` Tag-First Deserialization
Remove `#[serde(untagged)]` from the `Element` enum and implement `Deserialize` manually.

- **Observation**: Structured content in Yomichan dictionaries consistently places the `"tag"` field as the first property of the object.
- **Strategy**:
    1. Implement a `Visitor` that peeks at the first key.
    2. If the first key is `"tag"`, deserialize the value into an `HtmlTag` enum.
    3. Use the `HtmlTag` (e.g., `"br"`, `"ruby"`, `"a"`, `"div"`) to immediately dispatch to the specific variant struct (`LineBreak`, `UnstyledElement`, `LinkElement`, `StyledElement`).
    4. If `"tag"` is not the first field, use a lightweight temporary map (like `IndexMap<CompactString, Value>`) as a fallback, but optimize for the common "tag-first" case.

### 2.3. `MetaDataMatchType` Explicit Dispatch
Remove `#[serde(untagged)]` and the `UntaggedEnumVisitor` buffering.

- **Strategy**:
    1. Check the JSON token type.
    2. **String/Integer**: Map directly to `MetaDataMatchType::Frequency`.
    3. **Map**: Peek at the keys. 
        - `"frequency"` or `"value"` -> `Frequency`.
        - `"pitches"` -> `Pitch`.
        - `"transcriptions"` -> `Phonetic`.
    4. Use `serde-untagged` or a manual visitor to avoid `serde_json::Value` allocation for the entire object.

### 2.4. I/O and Parallelism
- **Parallelism**: Ensure the test runner uses `--features full` to enable `rayon` in `process_paths`.
- **BufReader**: Verify that `BufReader` is consistently used with a larger buffer size (e.g., 64KB or 128KB) to minimize the `uninlined_slow_read_byte` overhead.

## 3. Performance Goals
- **Target**: < 5 seconds for full import of `wty-es-es` dictionary.
- **Efficiency**: Reduce `__deserialize_content` contribution in the flamegraph from 44% to < 10%.

## 4. Verification Plan
- **Benchmark**: Run `./run.nu full` (with `rayon` and `simd`) and measure total time.
- **Flamegraph**: Generate a new `flamegraph.svg` to confirm the elimination of the `__deserialize_content` bottleneck.
- **Tests**: Ensure `cargo test` passes for all dictionary types.
