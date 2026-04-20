# Design Doc: Optimized Serde Deserializers for Yomichan Importer

Optimizing JSON deserialization by replacing `#[serde(untagged)]` enums and intermediate `serde_json::Value` buffering with explicit, type-aware `Visitor` implementations.

## 1. Problem Statement
The current importer takes ~7-8 seconds for a standard large dictionary. 
The flamegraph shows:
- ~40-50% of time in `serde::de::Deserializer::__deserialize_content`.
- Significant overhead from `#[serde(untagged)]` trial-and-error parsing.
- Expensive allocations and double-parsing in `ElementVisitor::visit_map` due to `serde_json::Value` buffering.

The goal is to reach **1-2 seconds** (and potentially sub-1s with Rayon/SIMD) by implementing "One-Pass" deserialization.

## 2. Proposed Changes

### 2.1. `ContentMatchType` One-Pass Visitor
Replace `#[serde(untagged)]` on `ContentMatchType` with a manual `Deserialize` implementation.

- **Logic**: Use `deserializer.deserialize_any(Visitor)`.
- **String**: Return `ContentMatchType::String(CompactString)`.
- **Array**: Return `ContentMatchType::Content(Vec<ContentMatchType>)`.
- **Map**: Peek for a `tag` or `type` field.
  - If `tag` exists: Return `ContentMatchType::Element(Box<Element>)`.
  - If `type == "structured-content"`: Return `ContentMatchType::Element` (nested structure).

### 2.2. `Element` Tag-First Deserializer
Eliminate the `serde_json::Map` buffering in `ElementVisitor`.

- **Strategy**: Instead of collecting all fields into a `Map` to find the `tag`, we will use a custom field visitor.
- **Optimization**: If the `tag` is not the first field, we use a lightweight `Content` enum (from `serde-value` or a custom event-based buffer) to store fields until the `tag` is found.
- **Dispatch**: Once the `tag` is identified (e.g., `"div"`, `"ruby"`, `"a"`), we immediately deserialize the remaining fields into the specific struct (`StyledElement`, `UnstyledElement`, etc.) without ever building a `serde_json::Value` tree.

### 2.3. `MetaDataMatchType` Explicit Dispatch
`MetaDataMatchType` is currently untagged and causes significant slowdown in `term_meta` processing.

- **Logic**: Use a visitor that checks the token type:
  - **Integer/String**: Directly return `MetaDataMatchType::Frequency`.
  - **Map**: Peek for unique keys:
    - `"pitches"` -> `MetaDataMatchType::Pitch`.
    - `"transcriptions"` -> `MetaDataMatchType::Phonetic`.
    - `"frequency"` or `"value"` -> `MetaDataMatchType::Frequency`.

### 2.4. Recursive Depth Handling
Ensure that the recursive nature of `StructuredContent` (often 5-10 levels deep) doesn't cause stack overflows by using `Box` where necessary (already present but should be verified).

## 3. Performance Goals
- **Target**: 1-2 seconds for full dictionary import.
- **Secondary Goal**: Sub-1s when combined with `rayon` (parallel file processing) and `simd-json`.

## 4. Verification Plan
- **Benchmark**: Run `./test.sh` and compare timings.
- **Integrity**: Ensure all existing tests in `src/test.rs` pass, confirming that the "One-Pass" logic handles all edge cases (like `tag` not being the first field in an object).
- **Flamegraph**: Generate a new `flamegraph.svg` to verify that `__deserialize_content` is no longer a major bottleneck.
