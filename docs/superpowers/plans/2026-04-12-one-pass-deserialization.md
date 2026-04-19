# One-Pass Deserialization Optimization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reduce dictionary import time from 12s to sub-5s by implementing one-pass JSON deserialization and enabling parallel processing.

**Architecture:** Replace `#[serde(untagged)]` and intermediate `serde_json::Value` buffering with custom `Visitor` implementations that dispatch based on peeking at the first field (like `"tag"` or `"type"`).

**Tech Stack:** Rust, Serde, Rayon, CompactStr, IndexMap.

---

### Task 1: Optimize `ContentMatchType` (One-Pass)

**Files:**
- Modify: `src/structured_content.rs`

- [ ] **Step 1: Update `ContentMatchType::visit_map` to avoid `serde_json::Value` buffering**

Replace the current `visit_map` in `src/structured_content.rs` with a version that peeks at the first key.

```rust
// src/structured_content.rs

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: de::MapAccess<'de>,
            {
                // Peek at the first key to determine dispatch
                let key: Option<CompactString> = map.next_key()?;
                
                match key.as_deref() {
                    Some("tag") => {
                        // It's an Element. We need to deserialize the rest of the element.
                        // Since we already consumed "tag", we need a way to pass it to the Element deserializer.
                        // For now, let's use a specialized seed or just handle it here if possible.
                        // Let's implement a helper that takes the first key-value pair.
                        let tag: HtmlTag = map.next_value()?;
                        let element = Element::deserialize_from_map(tag, map)?;
                        Ok(ContentMatchType::Element(Box::new(element)))
                    }
                    Some("type") => {
                        let type_val: CompactString = map.next_value()?;
                        if type_val == "structured-content" {
                             // Expect "content" key next
                             let content_key: Option<CompactString> = map.next_key()?;
                             if content_key.as_deref() == Some("content") {
                                 let content: ContentMatchType = map.next_value()?;
                                 // Consume any remaining fields (like "data" which is sometimes present but unused in wrapper)
                                 while let Some((_k, _v)) = map.next_entry::<IgnoredAny, IgnoredAny>()? {}
                                 
                                 Ok(ContentMatchType::Element(Box::new(Element::Styled(
                                    StyledElement {
                                        tag: HtmlTag::Div,
                                        content: Some(content),
                                        ..Default::default()
                                    },
                                ))))
                             } else {
                                 Err(de::Error::custom("expected 'content' field after 'type':'structured-content'"))
                             }
                        } else {
                            Err(de::Error::custom(format!("unknown type: {}", type_val)))
                        }
                    }
                    Some(k) => Err(de::Error::custom(format!("unexpected first key in ContentMatchType map: {}", k))),
                    None => Err(de::Error::custom("empty map for ContentMatchType")),
                }
            }
```

- [ ] **Step 2: Run tests to ensure no regressions**

Run: `cargo test`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/structured_content.rs
git commit -m "perf: implement one-pass visitor for ContentMatchType"
```

---

### Task 2: Optimize `Element` (Manual Tag-First Dispatch)

**Files:**
- Modify: `src/structured_content.rs`

- [ ] **Step 1: Remove `#[serde(untagged)]` from `Element` and implement `Deserialize` manually**

```rust
// src/structured_content.rs

impl<'de> Deserialize<'de> for Element {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ElementVisitor;
        impl<'de> Visitor<'de> for ElementVisitor {
            type Value = Element;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a structured content element map")
            }
            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where A: MapAccess<'de> {
                let key: Option<CompactString> = map.next_key()?;
                if key.as_deref() == Some("tag") {
                    let tag: HtmlTag = map.next_value()?;
                    Element::deserialize_from_map(tag, map)
                } else {
                    // Fallback for non-tag-first maps (rare in Yomichan but possible)
                    // ... implementation using a temporary buffer ...
                }
            }
        }
        deserializer.deserialize_map(ElementVisitor)
    }
}
```

- [ ] **Step 2: Implement `Element::deserialize_from_map` helper**

This helper will take the already-deserialized `tag` and the remaining `MapAccess`.

- [ ] **Step 3: Run tests**

Run: `cargo test`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/structured_content.rs
git commit -m "perf: implement tag-first dispatcher for Element"
```

---

### Task 3: Optimize `MetaDataMatchType` (Remove double-parsing)

**Files:**
- Modify: `src/dictionary_data.rs`

- [ ] **Step 1: Replace `UntaggedEnumVisitor` in `MetaDataMatchType::deserialize`**

Instead of buffering into `serde_json::Value`, use a visitor that peeks at keys.

```rust
// src/dictionary_data.rs

impl<'de> Deserialize<'de> for MetaDataMatchType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de> {
        struct MetaDataVisitor;
        impl<'de> Visitor<'de> for MetaDataVisitor {
             // ... peek at "frequency", "pitches", or "transcriptions" ...
        }
        deserializer.deserialize_any(MetaDataVisitor)
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/dictionary_data.rs
git commit -m "perf: remove double-parsing in MetaDataMatchType"
```

---

### Task 4: I/O & Parallelism Tuning

**Files:**
- Modify: `src/dictionary_importer.rs`
- Modify: `run.nu`

- [ ] **Step 1: Increase `BufReader` capacity**

```rust
// src/dictionary_importer.rs
let reader = BufReader::with_capacity(128 * 1024, file);
```

- [ ] **Step 2: Update `run.nu` to enable `full` feature by default or in a perf mode**

- [ ] **Step 3: Run benchmark with features enabled**

Run: `./run.nu full`
Expected: Execution time < 5s.

- [ ] **Step 4: Commit**

```bash
git add src/dictionary_importer.rs run.nu
git commit -m "perf: tune I/O buffers and enable parallel features"
```

---

### Task 5: Final Benchmark & Verification

- [ ] **Step 1: Generate a new flamegraph**

Run: `./run.nu full --profile` (assuming run.nu supports it or via cargo test)

- [ ] **Step 2: Compare results with original 12s benchmark**

- [ ] **Step 3: Final check of all tests**

Run: `cargo test --features full`
Expected: ALL PASS
