//! Contains the data structures for the structured content of the dictionary entries.

use std::{fmt, hash::Hash, marker::PhantomData};
use compact_str::{CompactString, ToCompactString};

use indexmap::IndexMap;
use serde::{
    Deserialize,
    Deserializer,
    Serialize,
    de::{self, MapAccess, SeqAccess, Visitor},
};
use serde_json::Value;
use serde_with::skip_serializing_none;

/// The object holding all html & information about an entry.
/// There is only one per term entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructuredContent {
    /// Identifier to mark the start of each entry's content.
    ///
    /// This should _always_ be `"type": "structured-content"` in the file.
    /// If not, the entry is not valid.
    #[serde(rename = "type")]
    pub content_type: CompactString,
    /// Contains the main content of the entry.
    /// _(see: [`ContentMatchType`] )_.
    ///
    /// Will _always_ be either an `Element (obj)` or a `Content (array)` _(ie: Never a String)`.
    pub content: ContentMatchType,
}

/// A match type to deserialize any `Content` type.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum ContentMatchType {
    /// A string.
    String(CompactString),
    /// A single html element.
    /// See: [`HtmlTag`].
    ///
    /// Most likely a [`HtmlTag::Anchor`] element.
    /// If so, the definition contains a reference to another entry.
    Element(Box<Element>),
    /// An array of html elements.
    /// See: [`HtmlTag`].
    ///
    Content(Vec<ContentMatchType>),
}

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

            fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
            where
                A: de::MapAccess<'de>,
            {
                let value: serde_json::Value =
                    de::Deserialize::deserialize(de::value::MapAccessDeserializer::new(map))?;

                if value.get("tag").is_some() {
                    let element: Element = serde_json::from_value(value).map_err(de::Error::custom)?;
                    Ok(ContentMatchType::Element(Box::new(element)))
                } else if value.get("type").and_then(|v| v.as_str()) == Some("structured-content") {
                    // This is a StructuredContent object inside a ContentMatchType
                    let sc: StructuredContent =
                        serde_json::from_value(value).map_err(de::Error::custom)?;
                    Ok(ContentMatchType::Element(Box::new(Element::Styled(
                        StyledElement {
                            tag: HtmlTag::Div,
                            content: Some(sc.content),
                            data: None,
                            style: None,
                            title: None,
                            open: None,
                            lang: None,
                        },
                    ))))
                } else {
                    Err(de::Error::custom(format!(
                        "Unknown map structure for ContentMatchType: {}",
                        value
                    )))
                }
            }
        }

        deserializer.deserialize_any(ContentMatchTypeVisitor)
    }
}

/// `yomichan_rs` unique struct.
/// The entire definition node tree parsed and inserted with correct formatting
/// in different ways for rendering.
///
/// # Fields
/// * `plain_text: String` - Usable in all programs for simple rendering of definitions
/// * `html: Option<String>` - Node tree parsed as html
#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct TermGlossaryContentGroup {
    // this is used for programs that cannot render html
    pub plain_text: CompactString,
    // this is used for programs that can render html (we ignore it for now)
    pub html: Option<CompactString>,
}

/// The type of a term glossary group.
#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub enum TermGlossaryGroupType {
    /// A content group.
    Content(TermGlossaryContentGroup),
    /// A deinflection group.
    Deinflection(TermGlossaryDeinflection),
}

/// A term glossary entry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TermGlossary {
    /// A content entry.
    Content(TermGlossaryContent),
    /// This is a tuple struct in js.
    /// If you see an `Array.isArray()` check on a [TermGlossary], its looking for this.
    Deinflection(TermGlossaryDeinflection),
}

impl From<TermGlossary> for TermGlossaryGroupType {
    fn from(value: TermGlossary) -> Self {
        match value {
            TermGlossary::Deinflection(d) => Self::Deinflection(d),
            TermGlossary::Content(ref c) => {
                let plain_text = match value {
                    TermGlossary::Content(ref c) => c.to_plain_text(),
                    _ => unreachable!(),
                };
                let group = TermGlossaryContentGroup {
                    plain_text: plain_text.into(),
                    html: None,
                };
                Self::Content(group)
            }
        }
    }
}

impl From<TermGlossaryContent> for TermGlossaryContentGroup {
    fn from(value: TermGlossaryContent) -> Self {
        let plain_text = value.to_plain_text();
        Self {
            plain_text: plain_text.into(),
            html: None,
        }
    }
}

impl TermGlossaryContent {
    pub fn to_plain_text(&self) -> CompactString {
        let mut buffer = CompactString::default();
        match self {
            Self::String(s) => {
                buffer.push_str(s);
            }
            Self::Tagged(tagged_content) => {
                Self::render_tagged_content(tagged_content, &mut buffer);
            }
        }
        buffer.trim().to_compact_string()
    }

    fn render_tagged_content(tagged: &TaggedContent, buffer: &mut CompactString) {
        match tagged {
            TaggedContent::Text { text } => {
                buffer.push_str(text);
            }
            TaggedContent::Image(image_element) => {
                if let Some(alt) = &image_element.alt {
                    buffer.push_str(alt);
                } else {
                    buffer.push_str(&CompactString::from(format!("[Image: {}]", image_element.path)));
                }
            }
            // This is the crucial part that contains the recursive tree.
            TaggedContent::StructuredContent { content } => {
                Self::render_content_match_type(content, buffer);
            }
        }
    }

    /// Helper that recursively renders any `ContentMatchType`.
    /// This is the main recursive dispatcher.
    fn render_content_match_type(content: &ContentMatchType, buffer: &mut CompactString) {
        match content {
            ContentMatchType::String(s) => {
                buffer.push_str(s);
            }
            ContentMatchType::Content(vec) => {
                for item in vec {
                    Self::render_content_match_type(item, buffer);
                }
            }
            ContentMatchType::Element(element) => {
                Self::render_element(element, buffer);
            }
        }
    }

    /// Renders a single, specific `Element` enum variant, applying formatting rules.
    fn render_element(element: &Element, buffer: &mut CompactString) {
        // --- 1. PRE-CONTENT FORMATTING (e.g., adding newlines for blocks) ---
        // We check the tag to see if it's a block-level element.
        let is_block = match element {
            Element::Styled(e) => matches!(
                e.tag,
                HtmlTag::Div
                    | HtmlTag::OrderedList
                    | HtmlTag::UnorderedList
                    | HtmlTag::ListItem
                    | HtmlTag::Details
                    | HtmlTag::TableRow
            ),
            // Treat whole tables and rows as blocks
            Element::Unstyled(e) => matches!(e.tag, HtmlTag::TableRow | HtmlTag::Table),
            // Should be handled by parent, but for safety
            Element::Table(e) => matches!(e.tag, HtmlTag::TableRow),
            Element::LineBreak(_) => true,
            _ => false,
        };

        if is_block {
            // Ensure we start on a new line, but don't add redundant newlines.
            if !buffer.is_empty() && !buffer.ends_with('\n') {
                buffer.push('\n');
            }
        }

        // --- 2. RENDER THE ELEMENT'S CONTENT RECURSIVELY ---
        match element {
            Element::UnknownString(s) => buffer.push_str(s),
            Element::Link(e) => {
                if let Some(content) = &e.content {
                    Self::render_content_match_type(content, buffer);
                }
            }
            Element::Styled(e) => {
                // Add indentation for list items
                if e.tag == HtmlTag::ListItem {
                    buffer.push_str("  - ");
                }
                if let Some(content) = &e.content {
                    Self::render_content_match_type(content, buffer);
                }
            }
            Element::Unstyled(e) => {
                if let Some(content) = &e.content {
                    Self::render_content_match_type(content, buffer);
                }
            }
            Element::Table(e) => {
                if let Some(content) = &e.content {
                    Self::render_content_match_type(content, buffer);
                }
                // Add a tab after table cells for spacing
                buffer.push('\t');
            }
            Element::LineBreak(_) => {
                // The newline is handled by the pre-formatting logic.
            }
            Element::Image(e) => {
                // For plain text, we can render the alt text or a placeholder.
                if let Some(alt) = &e.alt {
                    buffer.push_str(alt);
                } else {
                    buffer.push_str(&CompactString::from(format!("[Image: {}]", e.path)));
                }
            }
        }

        // --- 3. POST-CONTENT FORMATTING (e.g., adding newlines for blocks) ---
        if is_block {
            // After a block element, always ensure there's a newline.
            if !buffer.ends_with('\n') {
                buffer.push('\n');
            }
        }
    }
}

/// A deinflection entry in a term glossary.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TermGlossaryDeinflection {
    /// The form of the deinflection.
    pub form_of: CompactString,
    /// The rules of the deinflection.
    pub rules: Vec<CompactString>,
}

/// The content of a term glossary.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TermGlossaryContent {
    /// A string.
    String(CompactString),
    /// Tagged content.
    Tagged(TaggedContent),
}

/// Tagged content.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type")]
pub enum TaggedContent {
    /// Text content.
    #[serde(rename = "text")]
    Text { text: CompactString },
    /// Image content.
    #[serde(rename = "img")]
    Image(Box<ImageElement>),
    /// Structured content.
    #[serde(rename = "structured-content")]
    StructuredContent {
        // The payload is the value of the "content" field.
        #[serde(rename = "content")]
        content: ContentMatchType,
    },
}

impl<'de> Deserialize<'de> for TaggedContent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TaggedContentVisitor;

        impl<'de> Visitor<'de> for TaggedContentVisitor {
            type Value = TaggedContent;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str(
                    "a map with a 'type' key (JSON format) or a [tag, payload] sequence (MessagePack format)",
                )
            }

            /// Handles the MessagePack format: `["tag", payload]`
            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                // The first element is the tag string.
                let tag: String = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &"a [tag, payload] sequence"))?;

                // The second element is the payload, which depends on the tag.
                let content = match tag.as_str() {
                    "text" => {
                        let text: String = seq
                            .next_element()?
                            .ok_or_else(|| de::Error::invalid_length(1, &"a text payload"))?;
                        TaggedContent::Text { text: text.into() }
                    }
                    "img" => {
                        let image_payload: Box<ImageElement> = seq
                            .next_element()?
                            .ok_or_else(|| de::Error::invalid_length(1, &"an image payload"))?;
                        TaggedContent::Image(image_payload)
                    }
                    "structured-content" => {
                        let content: ContentMatchType = seq.next_element()?.ok_or_else(|| {
                            de::Error::invalid_length(1, &"a structured-content payload")
                        })?;
                        TaggedContent::StructuredContent { content }
                    }
                    _ => {
                        return Err(de::Error::unknown_variant(
                            &tag,
                            &["text", "img", "structured-content"],
                        ));
                    }
                };

                // Ensure there are no more elements in the sequence.
                if seq.next_element::<de::IgnoredAny>()?.is_some() {
                    return Err(de::Error::invalid_length(3, &self));
                }

                Ok(content)
            }

            /// Handles the JSON format: `{"type": "tag", ...}`
            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: de::MapAccess<'de>,
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
                        _ => {
                            let _: de::IgnoredAny = map.next_value()?;
                        }
                    }
                }

                let tag = tag.ok_or_else(|| de::Error::missing_field("type"))?;
                match tag.as_str() {
                    "text" => Ok(TaggedContent::Text {
                        text: text.ok_or_else(|| de::Error::missing_field("text"))?,
                    }),
                    "img" => Ok(TaggedContent::Image(
                        img.ok_or_else(|| de::Error::missing_field("img"))?,
                    )),
                    "structured-content" => Ok(TaggedContent::StructuredContent {
                        content: content.ok_or_else(|| de::Error::missing_field("content"))?,
                    }),
                    _ => Err(de::Error::unknown_variant(
                        &tag,
                        &["text", "img", "structured-content"],
                    )),
                }
            }
        }

        deserializer.deserialize_any(TaggedContentVisitor)
    }
}

/// A text entry in a term glossary.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct TermGlossaryText {
    /// The text of the glossary entry.
    pub text: CompactString,
}

/// The 'header', and `structured-content`
/// of a `term_bank_${i}.json` entry item.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TermEntryItem {
    /// The expression of the term.
    pub expression: CompactString,
    /// The reading of the term.
    pub reading: CompactString,
    /// The definition tags.
    pub def_tags: Option<CompactString>,
    /// The rules for the term.
    pub rules: CompactString,
    /// The score of the term.
    pub score: i128,
    /// The structured content of the term.
    pub structured_content: Vec<TermGlossary>,
    /// The sequence number of the term.
    pub sequence: i128,
    /// The term tags.
    pub term_tags: CompactString,
}

/// The rendering of an image.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageRendering {
    /// Automatic rendering.
    Auto,
    /// Pixelated rendering.
    Pixelated,
    /// Crisp edges rendering.
    CrispEdges,
}

/// The appearance of an image.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageAppearance {
    /// Automatic appearance.
    Auto,
    /// Monochrome appearance.
    Monochrome,
}

/// An HTML tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HtmlTag {
    /// A ruby tag.
    #[serde(rename = "ruby")]
    Ruby,
    /// A ruby text tag.
    #[serde(rename = "rt")]
    RubyText,
    /// A ruby parenthesis tag.
    #[serde(rename = "rp")]
    RubyParenthesis,
    /// A table tag.
    Table,
    /// A table data tag.
    #[serde(rename = "td")]
    TableData,
    /// A table header tag.
    #[serde(rename = "th")]
    TableHeader,
    /// A table body tag.
    #[serde(rename = "tb")]
    TableBody,
    /// A table footer tag.
    #[serde(rename = "tf")]
    TableFooter,
    /// A table row tag.
    #[serde(rename = "tr")]
    TableRow,
    /// An anchor tag.
    #[serde(rename = "a")]
    Anchor,
    /// A span tag.
    Span,
    /// A div tag.
    Div,
    /// An ordered list tag.
    #[serde(rename = "ol")]
    OrderedList,
    /// An unordered list tag.
    #[serde(rename = "ul")]
    UnorderedList,
    /// A list item tag.
    #[serde(rename = "li")]
    ListItem,
    /// A details tag.
    Details,
    /// A summary tag.
    Summary,
    /// A break tag.
    #[serde(rename = "br")]
    Break,
    /// An image tag.
    Img,
}

/// The vertical alignment of an element.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VerticalAlign {
    /// Baseline alignment.
    Baseline,
    /// Subscript alignment.
    Sub,
    /// Superscript alignment.
    Super,
    /// Text top alignment.
    #[serde(rename = "text-top")]
    TextTop,
    /// Text bottom alignment.
    #[serde(rename = "text-bottom")]
    TextBottom,
    /// Middle alignment.
    Middle,
    /// Top alignment.
    Top,
    /// Bottom alignment.
    Bottom,
}

/// The text decoration line of an element.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextDecorationLine {
    /// Underline.
    Underline,
    /// Overline.
    Overline,
    /// Line through.
    LineThrough,
}

/// The text decoration line of an element, or none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TextDecorationLineOrNone {
    /// No text decoration.
    None,
    /// A text decoration line.
    TextDecorationLine(TextDecorationLine),
}

/// The text decoration style of an element.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextDecorationStyle {
    /// Solid.
    Solid,
    /// Double.
    Double,
    /// Dotted.
    Dotted,
    /// Dashed.
    Dashed,
    /// Wavy.
    Wavy,
}

/// The font style of an element.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FontStyle {
    /// Normal.
    Normal,
    /// Italic.
    Italic,
}

/// The font weight of an element.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FontWeight {
    /// Normal.
    Normal,
    /// Bold.
    Bold,
}

/// The word break of an element.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WordBreak {
    /// Normal.
    Normal,
    /// Break all.
    BreakAll,
    /// Keep all.
    KeepAll,
}

/// The text alignment of an element.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextAlign {
    /// Start.
    Start,
    /// End.
    End,
    /// Left.
    Left,
    /// Right.
    Right,
    /// Center.
    Center,
    /// Justify.
    Justify,
    /// Justify all.
    JustifyAll,
    /// Match parent.
    MatchParent,
}

/// The size units of an element.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SizeUnits {
    /// Pixels.
    Px,
    /// Ems.
    Em,
}

/// The style of a structured content element.
#[skip_serializing_none]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuredContentStyle {
    font_style: Option<FontStyle>,
    font_weight: Option<FontWeight>,
    font_size: Option<CompactString>,
    color: Option<CompactString>,
    background: Option<CompactString>,
    background_color: Option<CompactString>,
    text_decoration_line: Option<TextDecorationLineOrNone>,
    text_decoration_style: Option<TextDecorationStyle>,
    text_decoration_color: Option<CompactString>,
    border_color: Option<CompactString>,
    border_style: Option<CompactString>,
    border_radius: Option<CompactString>,
    border_width: Option<CompactString>,
    clip_path: Option<CompactString>,
    vertical_align: Option<VerticalAlign>,
    text_align: Option<TextAlign>,
    text_emphasis: Option<CompactString>,
    text_shadow: Option<CompactString>,
    margin: Option<NumberOrString>,
    margin_top: Option<NumberOrString>,
    margin_left: Option<NumberOrString>,
    margin_right: Option<NumberOrString>,
    margin_bottom: Option<NumberOrString>,
    padding: Option<NumberOrString>,
    padding_top: Option<NumberOrString>,
    padding_left: Option<NumberOrString>,
    padding_right: Option<NumberOrString>,
    padding_bottom: Option<NumberOrString>,
    word_break: Option<WordBreak>,
    white_space: Option<CompactString>,
    cursor: Option<CompactString>,
    list_style_type: Option<CompactString>,
}

// daijisen: ~6.35s WITHOUT custom deserialization.
// daijisen: ~7.13 WITH custom deserialization.

struct ElementVisitor;

impl<'de> Visitor<'de> for ElementVisitor {
    type Value = Element;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter
            .write_str("a map with a 'tag' key (for JSON) or a sequence/tuple (for MessagePack)")
    }

    // This method will be called by `rmp_serde` when it sees an array.
    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        // Read the tag first.
        let tag: String = seq
            .next_element()?
            .ok_or_else(|| de::Error::invalid_length(0, &self))?;

        match tag.as_str() {
            "a" => Ok(Element::Link(de::Deserialize::deserialize(
                de::value::SeqAccessDeserializer::new(seq),
            )?)),
            "div" | "span" | "ol" | "ul" | "li" | "details" | "summary" => {
                Ok(Element::Styled(de::Deserialize::deserialize(
                    de::value::SeqAccessDeserializer::new(seq),
                )?))
            }
            "ruby" | "rt" | "rp" | "t" | "table" | "thead" | "tbody" | "tfoot" | "tr" | "tb"
            | "tf" => Ok(Element::Unstyled(de::Deserialize::deserialize(
                de::value::SeqAccessDeserializer::new(seq),
            )?)),
            "td" | "th" => Ok(Element::Table(de::Deserialize::deserialize(
                de::value::SeqAccessDeserializer::new(seq),
            )?)),
            "br" => Ok(Element::LineBreak(de::Deserialize::deserialize(
                de::value::SeqAccessDeserializer::new(seq),
            )?)),
            "img" => Ok(Element::Image(de::Deserialize::deserialize(
                de::value::SeqAccessDeserializer::new(seq),
            )?)),
            unknown_tag => {
                let known_variants = &[
                    "a", "div", "span", "ol", "ul", "li", "details", "summary", "ruby", "rt", "rp",
                    "t", "table", "thead", "tbody", "tfoot", "tr", "tb", "tf", "td", "th", "br",
                    "img",
                ];
                Err(de::Error::unknown_variant(unknown_tag, known_variants))
            }
        }
    }

    // This method will be called by `serde_json` when it sees an object.
    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        // For maps, we need to find the "tag" field.
        // If it's not the first field, we have to buffer the other fields.
        // However, for performance, we can try to optimize for the common case where tag is first.
        let mut tag = None;
        let mut fields = serde_json::Map::new();

        while let Some(key) = map.next_key::<String>()? {
            if key == "tag" {
                tag = Some(map.next_value::<String>()?);
            } else {
                fields.insert(key, map.next_value()?);
            }
        }

        let tag_str = tag.ok_or_else(|| de::Error::missing_field("tag"))?;
        let value = Value::Object(fields);

        // Here we still use Value for the remaining fields of the map to avoid complex buffering,
        // but we've avoided double-parsing the tag and the outer structure.
        // Further optimization can be done in Task 2.
        match tag_str.as_str() {
            "a" => Ok(Element::Link(
                LinkElement::deserialize_with_tag(tag_str, value).map_err(de::Error::custom)?,
            )),
            "div" | "span" | "ol" | "ul" | "li" | "details" | "summary" => Ok(Element::Styled(
                StyledElement::deserialize_with_tag(tag_str, value).map_err(de::Error::custom)?,
            )),
            "ruby" | "rt" | "rp" | "t" | "table" | "thead" | "tbody" | "tfoot" | "tr" | "tb"
            | "tf" => Ok(Element::Unstyled(
                UnstyledElement::deserialize_with_tag(tag_str, value).map_err(de::Error::custom)?,
            )),
            "td" | "th" => Ok(Element::Table(
                TableElement::deserialize_with_tag(tag_str, value).map_err(de::Error::custom)?,
            )),
            "br" => Ok(Element::LineBreak(
                LineBreak::deserialize_with_tag(tag_str, value).map_err(de::Error::custom)?,
            )),
            "img" => Ok(Element::Image(
                ImageElement::deserialize_with_tag(tag_str, value).map_err(de::Error::custom)?,
            )),
            unknown_tag => {
                let known_variants = &[
                    "a", "div", "span", "ol", "ul", "li", "details", "summary", "ruby", "rt", "rp",
                    "t", "table", "thead", "tbody", "tfoot", "tr", "tb", "tf", "td", "th", "br",
                    "img",
                ];
                Err(de::Error::unknown_variant(unknown_tag, known_variants))
            }
        }
    }
}

impl<'de> Deserialize<'de> for Element {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // The deserializer now dispatches to the correct visitor method
        // based on the data format it is reading.
        deserializer.deserialize_any(ElementVisitor)
    }
}

/// Represents All `Content` elements that can
/// appear within a `"content":` section.
#[skip_serializing_none]
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Element {
    /// An unknown string.
    UnknownString(String),
    /// A link element.
    Link(LinkElement),
    /// A styled element.
    Styled(StyledElement),
    /// An unstyled element.
    Unstyled(UnstyledElement),
    /// A table element.
    Table(TableElement),
    /// A line break element.
    LineBreak(LineBreak),
    /// An image element.
    Image(ImageElement),
}

/// This element doesn't support children or support language.
#[skip_serializing_none]
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineBreak {
    /// The `LineBreak`' tag is:
    /// [`HtmlTag::Break`] | `"br"`.
    pub tag: HtmlTag,
    data: Option<IndexMap<CompactString, CompactString>>,
}

/// An unstyled element.
#[skip_serializing_none]
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnstyledElement {
    /// `UnstyledElements`'s' tags could be the following:
    ///
    /// [`HtmlTag::Ruby`],
    /// [`HtmlTag::RubyText`],
    /// [`HtmlTag::RubyParenthesis`],
    /// [`HtmlTag::Table`],
    /// [`HtmlTag::TableHeader`],
    /// [`HtmlTag::TableBody`],
    /// [`HtmlTag::TableFooter`],
    /// [`HtmlTag::TableRow`].
    pub tag: HtmlTag,
    /// The content of the element.
    pub content: Option<ContentMatchType>,
    /// The data of the element.
    pub data: Option<IndexMap<String, String>>,
    /// Defines the language of an element in the format defined by RFC 5646.
    lang: Option<CompactString>,
}

/// A table element.
#[skip_serializing_none]
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableElement {
    /// `TableElement`'s tags could be the following:
    ///
    /// [`HtmlTag::TableData`],
    /// [`HtmlTag::TableHeader`].
    pub tag: HtmlTag,
    /// The content of the element.
    pub content: Option<ContentMatchType>,
    /// The data of the element.
    pub data: Option<IndexMap<String, String>>,
    /// The column span of the element.
    pub col_span: Option<u16>,
    /// The row span of the element.
    pub row_span: Option<u16>,
    /// The style of the element.
    pub style: Option<StructuredContentStyle>,
    /// Defines the language of an element in the format defined by RFC 5646.
    lang: Option<CompactString>,
}

impl<'de> Deserialize<'de> for TableElement {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TableElementVisitor;

        impl<'de> Visitor<'de> for TableElementVisitor {
            type Value = TableElement;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a sequence for a TableElement")
            }

            // This is the method that will be called for your MessagePack data
            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                // Field 1: Tag (required, always first)
                let tag: HtmlTag = seq
                    .next_element()? 
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;

                // Now, we handle the rest of the fields which might be optional or in any order.
                // The most robust way is to read them all as generic values and then pick them apart.
                let mut content = None;
                let mut row_span = None;
                let mut col_span = None;
                let mut style = None;
                let mut data = None;

                // Loop through the remaining elements in the sequence
                while let Some(value) = seq.next_element::<serde_json::Value>()? {
                    // Try to see if the value is a number (for row_span/col_span)
                    if let Some(num) = value.as_u64() {
                        // Business rule: assume the first number is row_span, second is col_span
                        if row_span.is_none() {
                            row_span = Some(num as u16);
                        } else if col_span.is_none() {
                            col_span = Some(num as u16);
                        }
                        continue; // Go to next item in sequence
                    }

                    if value.is_object() {
                        // Try to see if it's a style object
                        if let Ok(s) = serde_json::from_value::<StructuredContentStyle>(value.clone()) {
                            style = Some(s);
                            continue;
                        }

                        // Try to see if it's a data object
                        if let Ok(d) = serde_json::from_value::<IndexMap<String, String>>(value.clone()) {
                            data = Some(d);
                            continue;
                        }
                    }

                    // If it's none of the above, it must be the content.
                    // We can only have one content field.
                    if content.is_none() {
                        content = Some(serde_json::from_value(value).map_err(de::Error::custom)?);
                    }
                }

                Ok(TableElement {
                    tag,
                    content,
                    data,
                    col_span,
                    row_span,
                    style,
                    lang: None, // lang is not in the sequence format
                })
            }

            // OPTIONAL: To maintain compatibility with JSON map format if needed
            fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
            where
                A: de::MapAccess<'de>,
            {
                // This will deserialize from the map-based JSON format
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Helper {
                    tag: HtmlTag,
                    content: Option<ContentMatchType>,
                    data: Option<IndexMap<CompactString, CompactString>>,
                    col_span: Option<u16>,
                    row_span: Option<u16>,
                    style: Option<StructuredContentStyle>,
                    lang: Option<CompactString>,
                }

                let helper = Helper::deserialize(de::value::MapAccessDeserializer::new(map))?;
                Ok(TableElement {
                    tag: helper.tag,
                    content: helper.content,
                    data: helper.data.map(|m| m.into_iter().map(|(k, v)| (k.into(), v.into())).collect()),
                    col_span: helper.col_span,
                    row_span: helper.row_span,
                    style: helper.style,
                    lang: helper.lang.map(Into::into),
                })
            }
        }

        // This allows Serde to call visit_seq for sequences and visit_map for maps
        deserializer.deserialize_any(TableElementVisitor)
    }
}

/// A styled element.
#[skip_serializing_none]
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StyledElement {
    /// `StyledElement`'s tags are:
    ///
    /// [`HtmlTag::Span`],
    /// [`HtmlTag::Div`],
    /// [`HtmlTag::OrderedList`],
    /// [`HtmlTag::UnorderedList`],
    /// [`HtmlTag::ListItem`],
    /// [`HtmlTag::Details`],
    /// [`HtmlTag::Summary`].
    pub tag: HtmlTag,
    /// The content of the element.
    pub content: Option<ContentMatchType>,
    /// The data of the element.
    pub data: Option<IndexMap<String, String>>,
    /// The style of the element.
    pub style: Option<StructuredContentStyle>,
    /// Hover text for the element.
    pub title: Option<CompactString>,
    /// Whether the element is open.
    pub open: Option<bool>,
    /// Defines the language of an element in the format defined by RFC 5646.
    lang: Option<CompactString>,
}

/// A generic visitor that can deserialize a map directly, or convert a
/// sequence into a temporary map-like `Value` and deserialize from that.
pub struct FlexibleElementVisitor<T> {
    _marker: PhantomData<T>,
}

impl<T> FlexibleElementVisitor<T> {
    pub fn new() -> Self {
        FlexibleElementVisitor {
            _marker: PhantomData,
        }
    }
}

impl<'de, T> Visitor<'de> for FlexibleElementVisitor<T>
where
    T: de::DeserializeOwned, // The target type (e.g., TableElement)
{
    type Value = T;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a map or a sequence representing an element")
    }

    /// This is called for your database's sequence format.
    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        // 1. Build a serde_json::Map from the sequence. This is the adapter logic.
        let mut map = serde_json::Map::new();

        // Tag is always first and required.
        let tag: String = seq
            .next_element()? 
            .ok_or_else(|| de::Error::invalid_length(0, &"tag"))?;
        map.insert("tag".to_string(), Value::String(tag));

        // Loop through the rest of the optional, unordered fields.
        let mut content_val: Option<Value> = None;
        while let Some(value) = seq.next_element::<Value>()? {
            // Check for number types (rowSpan, colSpan)
            if value.is_u64() {
                // Heuristic: first number is rowSpan, second is colSpan.
                if !map.contains_key("rowSpan") {
                    map.insert("rowSpan".to_string(), value);
                } else if !map.contains_key("colSpan") {
                    map.insert("colSpan".to_string(), value);
                }
                continue;
            }

            // Heuristic: A map is likely the 'data' field.
            if value.is_object() && !map.contains_key("data") {
                map.insert("data".to_string(), value);
                continue;
            }

            // Heuristic: An array could be 'style' or 'content'.
            // This is the trickiest part. A simple rule might be:
            // if it's an array of objects/strings, it's content.
            // if it's an array of simple values/specific objects, it's style.
            // For now, let's assume anything that isn't a known attribute is content.
            if content_val.is_none() {
                content_val = Some(value);
            } else if !map.contains_key("style") {
                // If content is already taken, this might be style.
                map.insert("style".to_string(), value);
            }
        }

        if let Some(content) = content_val {
            map.insert("content".to_string(), content);
        }

        // 2. Deserialize the target type T from the map we just built.
        T::deserialize(Value::Object(map)).map_err(de::Error::custom)
    }

    /// This is called for your JSON file's map format.
    fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        // Since the input is already a map, we can deserialize directly.
        T::deserialize(de::value::MapAccessDeserializer::new(map))
    }
}

/// A link element.
#[skip_serializing_none]
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkElement {
    /// The `LinkElement`'s tag is:
    ///
    /// [`HtmlTag::Anchor`] | `"a"`.
    pub tag: HtmlTag,
    /// The content of the element.
    pub content: Option<ContentMatchType>,
    /// The URL for the link.
    ///
    /// URLs starting with a `?` are treated as internal links to other dictionary content.
    pub href: CompactString,
    /// Defines the language of an element in the format defined by RFC 5646.
    ///
    ///yomichan_rs will currently only support `ja` & `ja-JP`.
    pub lang: Option<String>,
}

/// A number or a string.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NumberOrString {
    /// A number.
    Number(f64),
    /// A string.
    String(CompactString),
}

/// An image element.
#[skip_serializing_none]
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageElement {
    /// The tag of the element.
    pub tag: HtmlTag,
    /// This element doesn't support children.
    pub content: Option<()>,
    /// The vertical alignment of the image.
    pub vertical_align: Option<VerticalAlign>,
    /// Shorthand for border width, style, and color.
    pub border: Option<CompactString>,
    /// Roundness of the corners of the image's outer border edge.
    pub border_radius: Option<String>,
    /// The units for the width and height.
    pub size_units: Option<SizeUnits>,
    /// The data of the element.
    pub data: Option<IndexMap<String, String>>,
    /// Path to the image file in the archive.
    pub path: CompactString,
    /// Preferred width of the image.
    pub width: Option<f32>,
    /// Preferred height of the image.
    pub height: Option<f32>,
    /// Preferred width of the image.
    /// This is only used in the internal database.
    pub preferred_width: Option<f32>,
    /// Preferred height of the image.
    /// This is only used in the internal database.
    pub preferred_height: Option<f32>,
    /// Hover text for the image.
    pub title: Option<CompactString>,
    /// Alt text for the image.
    pub alt: Option<CompactString>,
    /// Description of the image.
    pub description: Option<CompactString>,
    /// Whether or not the image should appear pixelated at sizes larger than the image's native resolution.
    pub pixelated: Option<bool>,
    /// Controls how the image is rendered. The value of this field supersedes the pixelated field.
    pub image_rendering: Option<ImageRendering>,
    /// Controls the appearance of the image. The 'monochrome' value will mask the opaque parts of the image using the current text color.
    appearance: Option<ImageAppearance>,
    /// Whether or not a background color is displayed behind the image.
    background: Option<bool>,
    /// Whether or not the image is collapsed by default.
    collapsed: Option<bool>,
    /// Whether or not the image can be collapsed.
    collapsible: Option<bool>,
}

impl LinkElement {
    pub fn deserialize_with_tag(tag: String, mut value: Value) -> Result<Self, String> {
        if let Some(obj) = value.as_object_mut() {
            obj.insert("tag".to_string(), Value::String(tag));
        }
        serde_json::from_value(value).map_err(|e| e.to_string())
    }
}

impl StyledElement {
    pub fn deserialize_with_tag(tag: String, mut value: Value) -> Result<Self, String> {
        if let Some(obj) = value.as_object_mut() {
            obj.insert("tag".to_string(), Value::String(tag));
        }
        serde_json::from_value(value).map_err(|e| e.to_string())
    }
}

impl UnstyledElement {
    pub fn deserialize_with_tag(tag: String, mut value: Value) -> Result<Self, String> {
        if let Some(obj) = value.as_object_mut() {
            obj.insert("tag".to_string(), Value::String(tag));
        }
        serde_json::from_value(value).map_err(|e| e.to_string())
    }
}

impl TableElement {
    pub fn deserialize_with_tag(tag: String, mut value: Value) -> Result<Self, String> {
        if let Some(obj) = value.as_object_mut() {
            obj.insert("tag".to_string(), Value::String(tag));
        }
        serde_json::from_value(value).map_err(|e| e.to_string())
    }
}

impl LineBreak {
    pub fn deserialize_with_tag(tag: String, mut value: Value) -> Result<Self, String> {
        if let Some(obj) = value.as_object_mut() {
            obj.insert("tag".to_string(), Value::String(tag));
        }
        serde_json::from_value(value).map_err(|e| e.to_string())
    }
}

impl ImageElement {
    pub fn deserialize_with_tag(tag: String, mut value: Value) -> Result<Self, String> {
        if let Some(obj) = value.as_object_mut() {
            obj.insert("tag".to_string(), Value::String(tag));
        }
        serde_json::from_value(value).map_err(|e| e.to_string())
    }
}
//
// This section provides manual `Deserialize` implementations for all
// element structs. This is necessary because the database can store
// elements in a compact "sequence" format (e.g., ["span", ...])
// while the source JSON files use a "map" format (e.g., {"tag": "span", ...}).
//
// Each implementation uses a visitor that can handle BOTH formats,
// making the parsing logic robust across all data sources.
//
// ===================================================================

// --- Implementation for StyledElement ---

impl<'de> Deserialize<'de> for StyledElement {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct StyledElementVisitor;

        impl<'de> Visitor<'de> for StyledElementVisitor {
            type Value = StyledElement;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a map or sequence for a StyledElement")
            }

            // Handles JSON map format: {"tag": "span", ...}
            fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                // This part remains the same, it correctly handles JSON
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Helper {
                    tag: HtmlTag,
                    content: Option<ContentMatchType>,
                    data: Option<IndexMap<CompactString, CompactString>>,
                    style: Option<StructuredContentStyle>,
                    title: Option<String>,
                    open: Option<bool>,
                    lang: Option<CompactString>,
                }

                let helper = Helper::deserialize(de::value::MapAccessDeserializer::new(map))?;
                Ok(StyledElement {
                    tag: helper.tag,
                    content: helper.content,
                    data: helper.data.map(|m| m.into_iter().map(|(k, v)| (k.into(), v.into())).collect()),
                    style: helper.style,
                    title: helper.title.map(Into::into),
                    open: helper.open,
                    lang: helper.lang.map(Into::into),
                })
            }

            // Handles database sequence format: ["span", ...]
            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let tag: HtmlTag = seq
                    .next_element()? 
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;

                let mut content = None;
                let mut data = None;
                let mut style = None;
                let mut title = None;
                let mut open = None;

                while let Some(value) = seq.next_element::<Value>()? {
                    // --- START OF MODIFIED LOGIC ---

                    // Is it a boolean? -> `open`
                    if let Some(b) = value.as_bool() {
                        if open.is_none() {
                            open = Some(b);
                        }
                        continue;
                    }

                    if value.is_object() {
                        // Is it a map? -> `data`
                        if let Ok(d) = serde_json::from_value::<IndexMap<String, String>>(value.clone()) {
                            if data.is_none() {
                                data = Some(d);
                            }
                            continue;
                        }
                    }

                    // Is it a string that isn't content yet? -> `title`
                    if let Some(s) = value.as_str() {
                        if title.is_none() {
                            title = Some(CompactString::from(s));
                            // Don't assume it's title, could be content. We'll let content take priority.
                        }
                    }

                    // Is it an array? THIS IS THE NEW PART. It could be `style` or `content`.
                    if let Some(arr) = value.as_array() {
                        // Heuristic: If all elements are numbers or CSS-like strings, it's a style array.
                        let is_likely_style_array = arr.iter().all(|v| {
                            v.is_number() || (v.is_string() && !v.as_str().unwrap().contains('【'))
                        });

                        if is_likely_style_array && style.is_none() {
                            // Convert the style array into a style map that StructuredContentStyle understands.
                            // This is a simplified conversion. You may need to make this more specific
                            // based on the exact format of the style array.
                            let mut style_map = serde_json::Map::new();
                            if !arr.is_empty() {
                                style_map.insert("fontSize".to_string(), arr[0].clone());
                            }
                            if arr.len() > 1 {
                                style_map.insert("verticalAlign".to_string(), arr[1].clone());
                            }
                            if arr.len() > 2 {
                                style_map.insert("marginLeft".to_string(), arr[2].clone());
                            }
                            if arr.len() > 3 {
                                style_map.insert("marginRight".to_string(), arr[3].clone());
                            }
                            if let Ok(s) = serde_json::from_value(Value::Object(style_map)) {
                                style = Some(s);
                            }
                            continue;
                        }
                    }

                    // If none of the above, it must be content.
                    if content.is_none() {
                        content = Some(serde_json::from_value(value).map_err(de::Error::custom)?);
                        // If we just assigned content, what we thought was title might have been content.
                        if title.is_some() {
                            if let Some(ContentMatchType::String(s)) = &content {
                                if s == title.as_ref().unwrap() {
                                    title = None;
                                }
                            }
                        }
                    }
                    // --- END OF MODIFIED LOGIC ---
                }

                Ok(StyledElement {
                    tag,
                    content,
                    data,
                    style,
                    title,
                    open,
                    lang: None,
                })
            }
        }

        deserializer.deserialize_any(StyledElementVisitor)
    }
}

// --- Implementation for UnstyledElement ---

impl<'de> Deserialize<'de> for UnstyledElement {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct UnstyledElementVisitor;

        impl<'de> Visitor<'de> for UnstyledElementVisitor {
            type Value = UnstyledElement;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a map or sequence for an UnstyledElement")
            }

            fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Helper {
                    tag: HtmlTag,
                    content: Option<ContentMatchType>,
                    data: Option<IndexMap<CompactString, CompactString>>,
                    lang: Option<CompactString>,
                }
                let helper = Helper::deserialize(de::value::MapAccessDeserializer::new(map))?;
                Ok(UnstyledElement {
                    tag: helper.tag,
                    content: helper.content,
                    data: helper.data.map(|m| m.into_iter().map(|(k, v)| (k.into(), v.into())).collect()),
                    lang: helper.lang.map(Into::into),
                })
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let tag: HtmlTag = seq
                    .next_element()? 
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;

                let mut content = None;
                let mut data = None;

                while let Some(value) = seq.next_element::<Value>()? {
                    if value.is_object() {
                        if let Ok(d) = serde_json::from_value::<IndexMap<String, String>>(value.clone()) {
                            if data.is_none() {
                                data = Some(d);
                            }
                            continue;
                        }
                    }
                    if content.is_none() {
                        content = Some(serde_json::from_value(value).map_err(de::Error::custom)?);
                    }
                }

                Ok(UnstyledElement {
                    tag,
                    content,
                    data,
                    lang: None,
                })
            }
        }

        deserializer.deserialize_any(UnstyledElementVisitor)
    }
}

// --- Implementation for LinkElement ---

impl<'de> Deserialize<'de> for LinkElement {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct LinkElementVisitor;

        impl<'de> Visitor<'de> for LinkElementVisitor {
            type Value = LinkElement;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a map or sequence for a LinkElement")
            }

            fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Helper {
                    tag: HtmlTag,
                    content: Option<ContentMatchType>,
                    href: String,
                    lang: Option<CompactString>,
                }
                let helper = Helper::deserialize(de::value::MapAccessDeserializer::new(map))?;
                Ok(LinkElement {
                    tag: helper.tag,
                    content: helper.content,
                    href: helper.href.into(),
                    lang: helper.lang.map(Into::into),
                })
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let tag: HtmlTag = seq
                    .next_element()? 
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;

                let mut content = None;
                let mut href = None;

                // For a link, we expect two more items: the content and the href string.
                // We can distinguish them heuristically: hrefs often start with '?' or 'http'.
                while let Some(value) = seq.next_element::<Value>()? {
                    if let Some(s) = value.as_str() {
                        if s.starts_with('?') || s.starts_with("http") {
                            if href.is_none() {
                                href = Some(CompactString::from(s));
                            }
                            continue;
                        }
                    }
                    if content.is_none() {
                        content = Some(serde_json::from_value(value).map_err(de::Error::custom)?);
                    }
                }

                Ok(LinkElement {
                    tag,
                    content,
                    href: href.ok_or_else(|| de::Error::missing_field("href"))?,
                    lang: None,
                })
            }
        }

        deserializer.deserialize_any(LinkElementVisitor)
    }
}

// --- Implementation for ImageElement ---

impl<'de> Deserialize<'de> for ImageElement {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ImageElementVisitor;

        impl<'de> Visitor<'de> for ImageElementVisitor {
            type Value = ImageElement;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a map or sequence for an ImageElement")
            }

            // Handles JSON map format
            fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Helper {
                    tag: HtmlTag,
                    content: Option<()>,
                    vertical_align: Option<VerticalAlign>,
                    border: Option<String>,
                    border_radius: Option<CompactString>,
                    size_units: Option<SizeUnits>,
                    data: Option<IndexMap<CompactString, CompactString>>,
                    path: String,
                    width: Option<f32>,
                    height: Option<f32>,
                    preferred_width: Option<f32>,
                    preferred_height: Option<f32>,
                    title: Option<String>,
                    alt: Option<String>,
                    description: Option<String>,
                    pixelated: Option<bool>,
                    image_rendering: Option<ImageRendering>,
                    appearance: Option<ImageAppearance>,
                    background: Option<bool>,
                    collapsed: Option<bool>,
                    collapsible: Option<bool>,
                }

                let helper = Helper::deserialize(de::value::MapAccessDeserializer::new(map))?;
                Ok(ImageElement {
                    tag: helper.tag,
                    content: helper.content,
                    vertical_align: helper.vertical_align,
                    border: helper.border.map(Into::into),
                    border_radius: helper.border_radius.map(Into::into),
                    size_units: helper.size_units,
                    data: helper.data.map(|m| m.into_iter().map(|(k, v)| (k.into(), v.into())).collect()),
                    path: helper.path.into(),
                    width: helper.width,
                    height: helper.height,
                    preferred_width: helper.preferred_width,
                    preferred_height: helper.preferred_height,
                    title: helper.title.map(Into::into),
                    alt: helper.alt.map(Into::into),
                    description: helper.description.map(Into::into),
                    pixelated: helper.pixelated,
                    image_rendering: helper.image_rendering,
                    appearance: helper.appearance,
                    background: helper.background,
                    collapsed: helper.collapsed,
                    collapsible: helper.collapsible,
                })
            }

            // Handles database sequence format: ["img", "em", "path", 1.0, ...]
            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                // Based on the log, the sequence appears to be:
                // [tag, size_units, path, width, height, alt, appearance, pixelated, collapsed, collapsible]
                let tag: HtmlTag = seq
                    .next_element()? 
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;

                // The rest of the fields have a fixed order in this compact format.
                let size_units: Option<SizeUnits> = seq.next_element()?.unwrap_or(None);
                let path: CompactString = seq
                    .next_element()? 
                    .ok_or_else(|| de::Error::invalid_length(2, &self))?;
                let width: Option<f32> = seq.next_element()?.unwrap_or(None);
                let height: Option<f32> = seq.next_element()?.unwrap_or(None);
                let alt: Option<CompactString> = seq.next_element()?.unwrap_or(None);
                let appearance: Option<ImageAppearance> = seq.next_element()?.unwrap_or(None);
                let pixelated: Option<bool> = seq.next_element()?.unwrap_or(None);
                let collapsed: Option<bool> = seq.next_element()?.unwrap_or(None);
                let collapsible: Option<bool> = seq.next_element()?.unwrap_or(None);

                Ok(ImageElement {
                    tag,
                    path,
                    size_units,
                    width,
                    height,
                    alt,
                    appearance,
                    pixelated,
                    collapsed,
                    collapsible,
                    // Fields not present in the sequence format
                    content: None,
                    vertical_align: None,
                    border: None,
                    border_radius: None,
                    data: None,
                    preferred_width: None,
                    preferred_height: None,
                    title: None,
                    description: None,
                    image_rendering: None,
                    background: None,
                })
            }
        }

        deserializer.deserialize_any(ImageElementVisitor)
    }
}
// --- Implementation for LineBreak ---

impl<'de> Deserialize<'de> for LineBreak {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct LineBreakVisitor;

        impl<'de> Visitor<'de> for LineBreakVisitor {
            type Value = LineBreak;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a map or sequence for a LineBreak")
            }

            fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Helper {
                    tag: HtmlTag,
                    data: Option<IndexMap<CompactString, CompactString>>,
                }
                let helper = Helper::deserialize(de::value::MapAccessDeserializer::new(map))?;
                Ok(LineBreak {
                    tag: helper.tag,
                    data: helper.data.map(|m| m.into_iter().map(|(k, v)| (k.into(), v.into())).collect()),
                })
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let tag: HtmlTag = seq
                    .next_element()? 
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let data: Option<IndexMap<String, String>> = seq.next_element()?.unwrap_or(None);

                Ok(LineBreak { tag, data: data.map(|m| m.into_iter().map(|(k, v)| (k.into(), v.into())).collect()) })
            }
        }

        deserializer.deserialize_any(LineBreakVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_content_match_type_deserialization() {
        // Test String
        let json_str = json!("hello");
        let content: ContentMatchType = serde_json::from_value(json_str).unwrap();
        assert!(matches!(content, ContentMatchType::String(_)));

        // Test Element
        let json_el = json!({
            "tag": "span",
            "content": "inner"
        });
        let content: ContentMatchType = serde_json::from_value(json_el).unwrap();
        if let ContentMatchType::Element(el) = content {
            if let Element::Styled(s) = *el {
                assert_eq!(s.tag, HtmlTag::Span);
            } else {
                panic!("Expected Styled element");
            }
        } else {
            panic!("Expected Element variant");
        }

        // Test Content (Array)
        let json_arr = json!(["item1", {"tag": "br"}]);
        let content: ContentMatchType = serde_json::from_value(json_arr).unwrap();
        if let ContentMatchType::Content(vec) = content {
            assert_eq!(vec.len(), 2);
        } else {
            panic!("Expected Content variant");
        }
    }

    #[test]
    fn test_content_match_type_nested_structured_content() {
        // This is what the user wants to support
        let json_nested = json!({
            "type": "structured-content",
            "content": "nested string"
        });
        
        // This should now succeed with the manual implementation
        let content: ContentMatchType = serde_json::from_value(json_nested).unwrap();
        if let ContentMatchType::Element(el) = content {
            if let Element::Styled(s) = *el {
                assert_eq!(s.tag, HtmlTag::Div);
                if let Some(ContentMatchType::String(inner)) = &s.content {
                    assert_eq!(inner, "nested string");
                } else {
                    panic!("Expected inner content to be String");
                }
            } else {
                panic!("Expected Styled element (Div)");
            }
        } else {
            panic!("Expected Element variant");
        }
    }
}
