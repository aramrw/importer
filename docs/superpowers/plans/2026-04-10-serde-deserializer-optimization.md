# Serde Deserializer Optimization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Optimize the dictionary deserialization by replacing intermediate `serde_json::Value` objects and `#[serde(untagged)]` enums with explicit tag-based dispatching, resolving a 10-second performance bottleneck and fixing parser ambiguity ("breakage").

**Architecture:** We will implement manual `Deserialize` and `Visitor` traits that perform "Tag-First Dispatch." This ensures each JSON byte is parsed only once and that the correct variant is selected based on explicit tag matching rather than guesswork.

**Tech Stack:** Rust, Serde, compact_str, simd-json (optional feature).

---

### Task 1: Refactor `Element` and `TaggedContent` Dispatchers

**Files:**
- Modify: `src/structured_content.rs`

- [ ] **Step 1: Rewrite `TaggedContent::deserialize`**
Replace the current implementation that uses `serde_json::Value` with a tag-first dispatcher.

```rust
impl<'de> Deserialize<'de> for TaggedContent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TaggedContentVisitor;

        impl<'de> Visitor<'de> for TaggedContentVisitor {
            type Value = TaggedContent;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a map with 'type' or a sequence [tag, payload]")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let tag: String = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(0, &self))?;
                match tag.as_str() {
                    "text" => Ok(TaggedContent::Text { text: seq.next_element()?.ok_or_else(|| de::Error::invalid_length(1, &self))? }),
                    "img" => Ok(TaggedContent::Image(seq.next_element()?.ok_or_else(|| de::Error::invalid_length(1, &self))?)),
                    "structured-content" => Ok(TaggedContent::StructuredContent { content: seq.next_element()?.ok_or_else(|| de::Error::invalid_length(1, &self))? }),
                    _ => Err(de::Error::unknown_variant(&tag, &["text", "img", "structured-content"])),
                }
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut tag = None;
                let mut content = None;
                let mut text = None;
                let mut img = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "type" => tag = Some(map.next_value::<String>()?),
                        "content" => content = Some(map.next_value()?),
                        "text" => text = Some(map.next_value()?),
                        "img" => img = Some(map.next_value()?),
                        _ => { let _: de::IgnoredAny = map.next_value()?; }
                    }
                }

                let tag = tag.ok_or_else(|| de::Error::missing_field("type"))?;
                match tag.as_str() {
                    "text" => Ok(TaggedContent::Text { text: text.ok_or_else(|| de::Error::missing_field("text"))? }),
                    "img" => Ok(TaggedContent::Image(img.ok_or_else(|| de::Error::missing_field("img"))?)),
                    "structured-content" => Ok(TaggedContent::StructuredContent { content: content.ok_or_else(|| de::Error::missing_field("content"))? }),
                    _ => Err(de::Error::unknown_variant(&tag, &["text", "img", "structured-content"])),
                }
            }
        }
        deserializer.deserialize_any(TaggedContentVisitor)
    }
}
```

- [ ] **Step 2: Rewrite `Element::deserialize` and `deserialize_element_from_value`**
Completely remove `deserialize_element_from_value` and rewrite `Element::deserialize` to dispatch without intermediate values.

```rust
impl<'de> Deserialize<'de> for Element {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ElementVisitor;
        impl<'de> Visitor<'de> for ElementVisitor {
            type Value = Element;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a map with 'tag' or a sequence starting with tag")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where A: SeqAccess<'de> {
                let tag: String = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(0, &self))?;
                match tag.as_str() {
                    "a" => Ok(Element::Link(de::Deserialize::deserialize(de::value::SeqAccessDeserializer::new(seq))?)),
                    "div" | "span" | "ol" | "ul" | "li" | "details" | "summary" => Ok(Element::Styled(de::Deserialize::deserialize(de::value::SeqAccessDeserializer::new(seq))?)),
                    "ruby" | "rt" | "rp" | "t" | "table" | "thead" | "tbody" | "tfoot" | "tr" | "tb" | "tf" => Ok(Element::Unstyled(de::Deserialize::deserialize(de::value::SeqAccessDeserializer::new(seq))?)),
                    "td" | "th" => Ok(Element::Table(de::Deserialize::deserialize(de::value::SeqAccessDeserializer::new(seq))?)),
                    "br" => Ok(Element::LineBreak(de::Deserialize::deserialize(de::value::SeqAccessDeserializer::new(seq))?)),
                    "img" => Ok(Element::Image(de::Deserialize::deserialize(de::value::SeqAccessDeserializer::new(seq))?)),
                    _ => Err(de::Error::unknown_variant(&tag, &["a", "div", "span", "ol", "ul", "li", "details", "summary", "ruby", "rt", "rp", "t", "table", "thead", "tbody", "tfoot", "tr", "tb", "tf", "td", "th", "br", "img"])),
                }
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where A: MapAccess<'de> {
                // To handle map, we might need a BufferedMap if 'tag' isn't first.
                // But for now, let's assume 'tag' is present.
                let mut tag = None;
                let mut fields = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if key == "tag" {
                        tag = Some(map.next_value::<String>()?);
                    } else {
                        fields.insert(key, map.next_value()?);
                    }
                }
                let tag = tag.ok_or_else(|| de::Error::missing_field("tag"))?;
                let value = serde_json::Value::Object(fields); // Fallback for map if needed, but let's try to pass 'tag' and remaining fields.
                // Optimization: If tag was found, we can use a custom deserializer for the remaining fields.
                match tag.as_str() {
                    "a" => Ok(Element::Link(LinkElement::deserialize_with_tag(tag, value).map_err(de::Error::custom)?)),
                    // ... repeat for all variants
                    _ => Err(de::Error::unknown_variant(&tag, &["..."])),
                }
            }
        }
        deserializer.deserialize_any(ElementVisitor)
    }
}
```

- [ ] **Step 3: Run the tests**
Run: `cargo test dict --release --features full`
Expected: Tests should pass, but performance might not be fully optimized yet.

- [ ] **Step 4: Commit**
```bash
git add src/structured_content.rs
git commit -m "perf: refactor Element and TaggedContent dispatchers to avoid intermediate Value where possible"
```

---

### Task 2: Optimize Specific Element Structs

**Files:**
- Modify: `src/structured_content.rs`

- [ ] **Step 1: Rewrite `StyledElement::deserialize`**
Remove `serde_json::Value` usage in both `visit_map` and `visit_seq`.

- [ ] **Step 2: Rewrite `TableElement::deserialize`**
Remove `serde_json::Value` usage in both `visit_map` and `visit_seq`.

- [ ] **Step 3: Rewrite `LinkElement::deserialize`**
Remove `serde_json::Value` usage in both `visit_map` and `visit_seq`.

- [ ] **Step 4: Rewrite `ImageElement::deserialize`**
Remove `serde_json::Value` usage in both `visit_map` and `visit_seq`.

- [ ] **Step 5: Run benchmarks**
Run: `cargo test with_pprof --release --features full`
Check `flamegraph.svg`. The `from_value` and `untagged` overhead should be significantly reduced.

- [ ] **Step 6: Commit**
```bash
git add src/structured_content.rs
git commit -m "perf: eliminate from_value from all Element structs"
```

---

### Task 3: Resolve `ContentMatchType` Ambiguity

**Files:**
- Modify: `src/structured_content.rs`

- [ ] **Step 1: Implement manual `Deserialize` for `ContentMatchType`**
Instead of `#[serde(untagged)]`, use a visitor that peeks at the type.

```rust
impl<'de> Deserialize<'de> for ContentMatchType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de> {
        struct ContentVisitor;
        impl<'de> Visitor<'de> for ContentVisitor {
            type Value = ContentMatchType;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string, an element object, or an array of content")
            }
            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E> where E: de::Error {
                Ok(ContentMatchType::String(v.into()))
            }
            fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error> where A: MapAccess<'de> {
                Ok(ContentMatchType::Element(Box::new(Element::deserialize(de::value::MapAccessDeserializer::new(map))?)))
            }
            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error> where A: SeqAccess<'de> {
                // This is the tricky part. Is it ["tag", ...] or [Content, Content, ...]?
                // Peek at the first element.
                let first: serde_json::Value = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(0, &self))?;
                if let Some(s) = first.as_str() {
                    if is_valid_html_tag(s) {
                        // It's an element!
                        return Ok(ContentMatchType::Element(Box::new(Element::deserialize_from_tag_and_seq(s, seq)?)));
                    }
                }
                // It's a list of content!
                let mut vec = Vec::new();
                vec.push(serde_json::from_value(first).map_err(de::Error::custom)?);
                while let Some(item) = seq.next_element()? {
                    vec.push(item);
                }
                Ok(ContentMatchType::Content(vec))
            }
        }
        deserializer.deserialize_any(ContentVisitor)
    }
}
```

- [ ] **Step 2: Run tests to verify "breakage" is fixed**
Run: `cargo test dict --release --features full`
Expected: All dictionaries should import correctly without "missing field" or "invalid variant" errors.

- [ ] **Step 3: Commit**
```bash
git add src/structured_content.rs
git commit -m "fix: resolve ContentMatchType ambiguity with explicit peek dispatch"
```

---

### Task 4: Optimize `MetaDataMatchType` in `dictionary_data.rs`

**Files:**
- Modify: `src/dictionary_data.rs`

- [ ] **Step 1: Rewrite `MetaDataMatchType::deserialize`**
Replace the current implementation that uses `serde_untagged` and `serde_json::Value` with a tag-first dispatcher that checks for "frequency", "value", "pitches", or "transcriptions" fields directly.

- [ ] **Step 2: Run final validation**
Run: `cargo test with_pprof --release --features full`
Compare the total time with the original 10s.

- [ ] **Step 3: Commit**
```bash
git add src/dictionary_data.rs
git commit -m "perf: optimize MetaDataMatchType deserialization"
```
