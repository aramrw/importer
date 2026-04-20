# Design Doc: Serde Deserializer Optimization for Yomichan Importer

Optimizing the JSON/database deserialization by replacing intermediate `serde_json::Value` objects and `#[serde(untagged)]` enums with explicit tag-based dispatching.

## Problem Statement
The current deserialization logic is the primary performance bottleneck, taking approximately 10 seconds for a large dictionary. The issues are:
1.  **Intermediate `Value` Objects**: `Deserialize` implementations frequently convert raw JSON into `serde_json::Value` (a full tree of allocations) before re-deserializing it via `from_value`.
2.  **`#[serde(untagged)]` Enums**: Enums like `Element` and `ContentMatchType` are untagged, causing Serde to try variants one by one. This is slow and prone to ambiguity errors ("breakage").
3.  **Redundant Allocations**: Recursive structured content leads to many small heap allocations (`Box<Element>`, `Vec<ContentMatchType>`).

## Proposed Changes

### 1. Explicit Tag Dispatching
Replace `#[serde(untagged)]` on `Element`, `ContentMatchType`, and `TermGlossary` with manual `Deserialize` implementations.
- **For Elements**: Read the `tag` field (Map) or the first element (Sequence). Match the tag string ("a", "div", "span", etc.) and dispatch directly to the corresponding struct's `Deserialize`.
- **For Content**: "Peek" at the first element of an array to distinguish between a single "sequence-style" element (e.g., `["span", ...]`) and a list of definitions (`Vec<ContentMatchType>`).

### 2. Eliminating `from_value`
Rewrite `Visitor` implementations to deserialize directly from the stream using `SeqAccess` and `MapAccess`.
- Pass the `Deserializer` directly to target structs like `LinkElement`, `StyledElement`, etc.
- If a tag field appears late in a JSON map, use a lightweight "BufferedField" approach to capture preceding fields without allocating a full `serde_json::Value` tree.

### 3. Optimization of Recursive Trees
- Use `CompactString` for all string fields to reduce heap allocations (already partially implemented).
- Review `Box<Element>` usage to see if any can be replaced with more efficient structures, though `Box` is required for recursion.

## Data Flow
1. `TermEntryItem` deserialization begins.
2. `structured_content` (Vec<TermGlossary>) starts deserializing.
3. `TermGlossary` dispatcher reads the tag or checks if it's a string/object.
4. If it's `StructuredContent`, it recursively calls the `Element` dispatcher.
5. The `Element` dispatcher identifies the tag and routes the stream to the specific element struct (`StyledElement`, `LinkElement`, etc.).
6. The specific struct deserializes its fields directly from the JSON stream.

## Testing Strategy
1. **Performance Benchmark**: Run the `with_pprof` test before and after changes to verify the 10s bottleneck is resolved.
2. **Ambiguity Test**: Create a test case with overlapping fields (e.g., a `StyledElement` and `TableElement` with similar fields) to ensure the explicit dispatch correctly identifies them.
3. **Format Compatibility**: Verify that both the "map" format (JSON source) and the "sequence" format (database) are correctly parsed by the same visitors.
4. **Regression Testing**: Run existing dictionary import tests to ensure no breakage in data integrity.
