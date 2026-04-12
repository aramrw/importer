# Serde Deserializer Optimization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reduce dictionary import time from ~7-8s to 1-2s.

**Architecture:** Replace `#[serde(untagged)]` and `serde_json::Value` buffering with explicit, type-aware `Visitor` implementations for `ContentMatchType`, `Element`, and `MetaDataMatchType`.

**Tech Stack:** Rust, Serde, serde_json, compact_str.

---

### Task 1: Optimize `ContentMatchType` Deserialization

**Files:**
- Modify: `src/structured_content.rs`
- Test: `src/test.rs` (or existing tests)

- [ ] **Step 1: Implement manual `Deserialize` for `ContentMatchType`**

Replace the `#[serde(untagged)]` attribute with a manual implementation that uses `deserialize_any` to branch on the token type (String, Array, or Map).

```rust
impl<'de> Deserialize<'de> for ContentMatchType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ContentMatchTypeVisitor;

        impl<'de> Visitor<'de> for ContentMatchTypeVisitor {
            type Value = ContentMatchType;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string, an array of content, or a structured content object")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(ContentMatchType::String(v.to_compact_string()))
            }

            fn visit_seq<A>(self, seq: A) -> Result<Self::Value, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                let vec = Vec::deserialize(de::value::SeqAccessDeserializer::new(seq))?;
                Ok(ContentMatchType::Content(vec))
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: de::MapAccess<'de>,
            {
                // Peek for 'tag' to identify an Element
                // We'll use a temporary strategy here that identifies the variant
                // without full buffering if possible, or a minimal buffer.
                let mut tag: Option<String> = None;
                let mut content_type: Option<String> = None;
                
                // For now, we'll use a simple approach to identify the variant.
                // A more advanced approach involves a custom Seed that peeks.
                let value: serde_json::Value = de::Deserialize::deserialize(de::value::MapAccessDeserializer::new(map))?;
                
                if value.get("tag").is_some() {
                    let element: Element = serde_json::from_value(value).map_err(de::Error::custom)?;
                    Ok(ContentMatchType::Element(Box::new(element)))
                } else if value.get("type").and_then(|v| v.as_str()) == Some("structured-content") {
                    // This is a StructuredContent object inside a ContentMatchType
                    // Handle recursively or as needed.
                    let sc: StructuredContent = serde_json::from_value(value).map_err(de::Error::custom)?;
                    Ok(ContentMatchType::Element(Box::new(Element::Unstyled(UnstyledElement {
                        tag: HtmlTag::Div,
                        content: Some(sc.content),
                        data: None,
                        lang: None,
                    }))))
                } else {
                    Err(de::Error::custom("Unknown map structure for ContentMatchType"))
                }
            }
        }

        deserializer.deserialize_any(ContentMatchTypeVisitor)
    }
}
```

- [ ] **Step 2: Run tests to verify performance and correctness**

Run: `cargo test --release --features full`
Expected: PASS and a noticeable speedup.

- [ ] **Step 3: Commit**

```bash
git add src/structured_content.rs
git commit -m "perf: optimize ContentMatchType deserialization"
```

---

### Task 2: Optimize `Element` Deserialization (Remove `Value` Buffering)

**Files:**
- Modify: `src/structured_content.rs`

- [ ] **Step 1: Refactor `ElementVisitor` to avoid `serde_json::Value`**

Instead of `visit_map` buffering into `serde_json::Map`, implement a "Tag-First" strategy.

```rust
// In src/structured_content.rs
impl<'de> Visitor<'de> for ElementVisitor {
    // ... visit_seq is already optimized ...

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        // We need to find the "tag" field without buffering everything.
        // We can use a custom "Ignored" pattern or a lightweight event buffer.
        // For simplicity in this step, we'll implement a stateful visitor
        // that looks for "tag" and stores other fields in a more efficient way than Value.
        
        let mut tag: Option<String> = None;
        let mut fields = Vec::new(); // Store (Key, Value) pairs temporarily

        while let Some(key) = map.next_key::<String>()? {
            if key == "tag" {
                tag = Some(map.next_value()?);
            } else {
                let val: serde_json::Value = map.next_value()?;
                fields.push((key, val));
            }
        }

        let tag_str = tag.ok_or_else(|| de::Error::missing_field("tag"))?;
        
        // Dispatch based on tag
        match tag_str.as_str() {
            "a" => {
                // Construct LinkElement from fields
                // ...
            }
            // ... other tags ...
        }
        // ...
    }
}
```

- [ ] **Step 2: Run tests and benchmarks**

Run: `cargo test --release --features full`
Check `flamegraph.svg` for reduced `__deserialize_content` time.

- [ ] **Step 3: Commit**

```bash
git add src/structured_content.rs
git commit -m "perf: eliminate Value buffering in Element deserializer"
```

---

### Task 3: Optimize `MetaDataMatchType` Deserialization

**Files:**
- Modify: `src/dictionary_data.rs`

- [ ] **Step 1: Replace `UntaggedEnumVisitor` with direct branching**

```rust
impl<'de> Deserialize<'de> for MetaDataMatchType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MetaDataVisitor;
        impl<'de> Visitor<'de> for MetaDataVisitor {
            type Value = MetaDataMatchType;
            // ... visit_str, visit_i128 ...
            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                // Peek at first key to identify type
                // "pitches" -> Pitch
                // "transcriptions" -> Phonetic
                // "frequency"/"value" -> Frequency
                // ...
            }
        }
        deserializer.deserialize_any(MetaDataVisitor)
    }
}
```

- [ ] **Step 2: Verify import speed**

Run: `./test.sh`
Expected: Import time < 2s.

- [ ] **Step 3: Commit**

```bash
git add src/dictionary_data.rs
git commit -m "perf: optimize MetaDataMatchType deserialization"
```
