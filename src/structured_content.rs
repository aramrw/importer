//! Contains the data structures for the structured content of the dictionary entries.

use compact_str::{CompactString, ToCompactString};
use std::{fmt, hash::Hash, marker::PhantomData};

use indexmap::IndexMap;
use serde::{
    Deserialize, Deserializer, Serialize,
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

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: de::MapAccess<'de>,
            {
                let mut tag = None;
                let mut ty = None;
                let mut buffered_fields: IndexMap<CompactString, Value> = IndexMap::new();

                while let Some(key) = map.next_key::<CompactString>()? {
                    match key.as_str() {
                        "tag" => {
                            let t: HtmlTag = map.next_value()?;
                            if buffered_fields.is_empty() && ty.is_none() {
                                // Fast path: nothing buffered yet
                                return Ok(ContentMatchType::Element(Box::new(
                                    Element::deserialize_from_map(t, map)?,
                                )));
                            }
                            tag = Some(t);
                        }
                        "type" => {
                            let t: CompactString = map.next_value()?;
                            if t == "structured-content" && buffered_fields.is_empty() && tag.is_none() {
                                // Fast path for structured-content
                                let mut content = None;
                                while let Some(k) = map.next_key::<CompactString>()? {
                                    if k == "content" {
                                        content = Some(map.next_value()?);
                                    } else {
                                        let _: de::IgnoredAny = map.next_value()?;
                                    }
                                }
                                let content =
                                    content.ok_or_else(|| de::Error::missing_field("content"))?;

                                return Ok(ContentMatchType::Element(Box::new(Element::Styled(
                                    StyledElement {
                                        tag: HtmlTag::Div,
                                        content: Some(content),
                                        data: None,
                                        style: None,
                                        title: None,
                                        open: None,
                                        lang: None,
                                    },
                                ))));
                            }
                            ty = Some(t);
                        }
                        _ => {
                            buffered_fields.insert(key, map.next_value()?);
                        }
                    }
                    if tag.is_some() || ty.is_some() {
                        break;
                    }
                }

                // If we are here, we either found tag/type late, or we finished the map.
                // Continue consuming if not finished.
                while let Some(key) = map.next_key::<CompactString>()? {
                    match key.as_str() {
                        "tag" => tag = tag.or(Some(map.next_value()?)),
                        "type" => ty = ty.or(Some(map.next_value()?)),
                        _ => {
                            buffered_fields.insert(key, map.next_value()?);
                        }
                    }
                }

                // Handle what we found
                if let Some(t) = tag {
                    let element = Element::deserialize_from_buffered_map(t, buffered_fields)
                        .map_err(de::Error::custom)?;
                    Ok(ContentMatchType::Element(Box::new(element)))
                } else if ty.as_deref() == Some("structured-content") {
                    let content_val = buffered_fields
                        .swap_remove("content")
                        .ok_or_else(|| de::Error::missing_field("content"))?;
                    let content: ContentMatchType =
                        serde_json::from_value(content_val).map_err(de::Error::custom)?;
                    Ok(ContentMatchType::Element(Box::new(Element::Styled(
                        StyledElement {
                            tag: HtmlTag::Div,
                            content: Some(content),
                            data: None,
                            style: None,
                            title: None,
                            open: None,
                            lang: None,
                        },
                    ))))
                } else {
                    Err(de::Error::custom(
                        "Unknown map structure for ContentMatchType",
                    ))
                }
            }
        }

        deserializer.deserialize_any(ContentMatchTypeVisitor)
    }
}

/// `yomichan_rs` unique struct.
/// Stores the definition rendered as HTML, with on-demand zero-regex stripping
/// to plain text via [`TermGlossaryContentGroup::to_plain_text`].
///
/// # Fields
/// * `html: CompactString` - Node tree parsed as HTML
#[derive(Clone, PartialEq, Serialize, Deserialize, Debug, Eq, Hash)]
pub struct TermGlossaryContentGroup {
    /// The definition rendered as HTML.
    pub html: CompactString,
}

impl TermGlossaryContentGroup {
    /// Returns the HTML representation of this definition.
    #[inline]
    pub fn as_html(&self) -> &str {
        &self.html
    }

    /// Strips all HTML tags and unescapes HTML entities in a single pass,
    /// preserving block newlines, list item bullets (`  - `), table cell tabs (`\t`),
    /// and image `alt` / `[Image: path]` placeholders.
    #[inline]
    pub fn to_plain_text(&self) -> CompactString {
        strip_html_to_plain_text(&self.html)
    }

    /// Alias for [`Self::to_plain_text`].
    #[inline]
    pub fn plain_text(&self) -> CompactString {
        self.to_plain_text()
    }
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
            TermGlossary::Content(c) => Self::Content(TermGlossaryContentGroup::from(c)),
        }
    }
}

impl From<TermGlossaryContent> for TermGlossaryContentGroup {
    fn from(value: TermGlossaryContent) -> Self {
        Self {
            html: value.to_html(),
        }
    }
}

/// Fast single-pass HTML-to-plain-text converter for rendered definitions.
/// Preserves block-level line breaks, `<br>`, `<li>` bullets (`  - `),
/// `<td>`/`<th>` cell tabs (`\t`), `<img alt="..." src="...">` text, and unescapes HTML entities.
pub fn strip_html_to_plain_text(html: &str) -> CompactString {
    let mut buffer = CompactString::with_capacity(html.len());
    let bytes = html.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        match bytes[i] {
            b'<' => {
                if let Some(rel_end) = bytes[i + 1..].iter().position(|&b| b == b'>') {
                    let tag_end = i + 1 + rel_end;
                    let tag_inner = &html[i + 1..tag_end];
                    handle_html_tag_for_plain_text(tag_inner, &mut buffer);
                    i = tag_end + 1;
                } else {
                    // Malformed '<' with no closing '>'
                    buffer.push('<');
                    i += 1;
                }
            }
            b'&' => {
                if let Some((unescaped, consumed)) = decode_html_entity(&html[i..]) {
                    buffer.push_str(&unescaped);
                    i += consumed;
                } else {
                    buffer.push('&');
                    i += 1;
                }
            }
            _ => {
                let start = i;
                i += 1;
                while i < len && bytes[i] != b'<' && bytes[i] != b'&' {
                    i += 1;
                }
                buffer.push_str(&html[start..i]);
            }
        }
    }

    buffer.trim().to_compact_string()
}

#[inline]
fn handle_html_tag_for_plain_text(tag_inner: &str, buffer: &mut CompactString) {
    let trimmed = tag_inner.trim();
    if trimmed.is_empty() {
        return;
    }

    let (is_closing, rest) = if let Some(stripped) = trimmed.strip_prefix('/') {
        (true, stripped.trim_start())
    } else {
        (false, trimmed)
    };

    let tag_name_end = rest
        .find(|c: char| c.is_ascii_whitespace() || c == '/')
        .unwrap_or(rest.len());
    let tag_name = &rest[..tag_name_end];

    if tag_name.eq_ignore_ascii_case("br") {
        if !buffer.is_empty() {
            buffer.push('\n');
        }
        return;
    }

    if !is_closing && tag_name.eq_ignore_ascii_case("img") {
        if let Some(alt) = extract_html_attr(rest, "alt") {
            buffer.push_str(&alt);
        } else if let Some(src) = extract_html_attr(rest, "src") {
            buffer.push_str("[Image: ");
            buffer.push_str(&src);
            buffer.push(']');
        }
        return;
    }

    let is_block = tag_name.eq_ignore_ascii_case("div")
        || tag_name.eq_ignore_ascii_case("ol")
        || tag_name.eq_ignore_ascii_case("ul")
        || tag_name.eq_ignore_ascii_case("li")
        || tag_name.eq_ignore_ascii_case("details")
        || tag_name.eq_ignore_ascii_case("tr")
        || tag_name.eq_ignore_ascii_case("table");

    if !is_closing {
        if is_block && !buffer.is_empty() && !buffer.ends_with('\n') {
            buffer.push('\n');
        }
        if tag_name.eq_ignore_ascii_case("li") {
            buffer.push_str("  - ");
        }
    } else {
        if tag_name.eq_ignore_ascii_case("td") || tag_name.eq_ignore_ascii_case("th") {
            buffer.push('\t');
        }
        if is_block && !buffer.is_empty() && !buffer.ends_with('\n') {
            buffer.push('\n');
        }
    }
}

fn extract_html_attr(tag_content: &str, attr_name: &str) -> Option<CompactString> {
    let mut rest = tag_content;
    while let Some(idx) = rest.find('=') {
        let before = rest[..idx].trim_end();
        let name_start = before
            .rfind(|c: char| c.is_ascii_whitespace())
            .map(|p| p + 1)
            .unwrap_or(0);
        let candidate = &before[name_start..];
        let after_eq = rest[idx + 1..].trim_start();

        if let Some(quote) = after_eq.chars().next() {
            if quote == '"' || quote == '\'' {
                let value_Part = &after_eq[1..];
                if let Some(end_quote) = value_Part.find(quote) {
                    if candidate.eq_ignore_ascii_case(attr_name) {
                        return Some(unescape_html_str(&value_Part[..end_quote]));
                    }
                    rest = &value_Part[end_quote + 1..];
                    continue;
                }
            }
        }
        rest = &rest[idx + 1..];
    }
    None
}

fn unescape_html_str(s: &str) -> CompactString {
    if !s.contains('&') {
        return CompactString::from(s);
    }
    let mut out = CompactString::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'&' {
            if let Some((decoded, consumed)) = decode_html_entity(&s[i..]) {
                out.push_str(&decoded);
                i += consumed;
                continue;
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

#[inline]
fn decode_html_entity(s: &str) -> Option<(CompactString, usize)> {
    let semi = s.as_bytes().iter().take(12).position(|&b| b == b';')?;
    let entity = &s[1..semi];
    let consumed = semi + 1;
    let decoded = match entity {
        "amp" => CompactString::const_new("&"),
        "lt" => CompactString::const_new("<"),
        "gt" => CompactString::const_new(">"),
        "quot" => CompactString::const_new("\""),
        "apos" | "#39" | "#x27" | "#X27" => CompactString::const_new("'"),
        "nbsp" | "#160" | "#xa0" | "#xA0" => CompactString::const_new("\u{00A0}"),
        _ if entity.starts_with("#x") || entity.starts_with("#X") => {
            let code = u32::from_str_radix(&entity[2..], 16).ok()?;
            let ch = char::from_u32(code)?;
            let mut cs = CompactString::default();
            cs.push(ch);
            cs
        }
        _ if entity.starts_with('#') => {
            let code = entity[1..].parse::<u32>().ok()?;
            let ch = char::from_u32(code)?;
            let mut cs = CompactString::default();
            cs.push(ch);
            cs
        }
        _ => return None,
    };
    Some((decoded, consumed))
}

#[inline]
fn push_escaped_html_text(buffer: &mut CompactString, text: &str) {
    let bytes = text.as_bytes();
    let mut last = 0;
    for (i, &b) in bytes.iter().enumerate() {
        let replacement = match b {
            b'&' => "&amp;",
            b'<' => "&lt;",
            b'>' => "&gt;",
            b'\n' => "<br>",
            _ => continue,
        };
        if last < i {
            buffer.push_str(&text[last..i]);
        }
        buffer.push_str(replacement);
        last = i + 1;
    }
    if last < bytes.len() {
        buffer.push_str(&text[last..]);
    }
}

#[inline]
fn push_escaped_html_attr(buffer: &mut CompactString, val: &str) {
    let bytes = val.as_bytes();
    let mut last = 0;
    for (i, &b) in bytes.iter().enumerate() {
        let replacement = match b {
            b'&' => "&amp;",
            b'<' => "&lt;",
            b'>' => "&gt;",
            b'"' => "&quot;",
            b'\'' => "&#39;",
            _ => continue,
        };
        if last < i {
            buffer.push_str(&val[last..i]);
        }
        buffer.push_str(replacement);
        last = i + 1;
    }
    if last < bytes.len() {
        buffer.push_str(&val[last..]);
    }
}

#[inline]
fn push_data_attributes(
    buffer: &mut CompactString,
    data: &Option<IndexMap<CompactString, CompactString>>,
) {
    if let Some(map) = data {
        for (k, v) in map {
            buffer.push_str(" data-sc-");
            for ch in k.chars() {
                if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                    buffer.push(ch.to_ascii_lowercase());
                }
            }
            buffer.push_str("=\"");
            push_escaped_html_attr(buffer, v);
            buffer.push('"');
        }
    }
}

impl TermGlossaryContent {
    /// Renders the structured content tree into an HTML string.
    pub fn to_html(&self) -> CompactString {
        let mut buffer = CompactString::default();
        match self {
            Self::String(s) => {
                push_escaped_html_text(&mut buffer, s.trim());
            }
            Self::Tagged(tagged_content) => {
                Self::render_tagged_content_html(tagged_content, &mut buffer);
            }
        }
        buffer
    }

    fn render_tagged_content_html(tagged: &TaggedContent, buffer: &mut CompactString) {
        match tagged {
            TaggedContent::Text { text } => {
                push_escaped_html_text(buffer, text.trim());
            }
            TaggedContent::Image(image_element) => {
                Self::render_image_html(image_element, buffer);
            }
            TaggedContent::StructuredContent { content } => {
                Self::render_content_match_type_html(content, buffer);
            }
        }
    }

    fn render_content_match_type_html(content: &ContentMatchType, buffer: &mut CompactString) {
        match content {
            ContentMatchType::String(s) => {
                push_escaped_html_text(buffer, s);
            }
            ContentMatchType::Content(vec) => {
                for item in vec {
                    Self::render_content_match_type_html(item, buffer);
                }
            }
            ContentMatchType::Element(element) => {
                Self::render_element_html(element, buffer);
            }
        }
    }

    fn render_element_html(element: &Element, buffer: &mut CompactString) {
        match element {
            Element::UnknownString(s) => {
                push_escaped_html_text(buffer, s);
            }
            Element::LineBreak(lb) => {
                buffer.push_str("<br");
                push_data_attributes(buffer, &lb.data);
                buffer.push('>');
            }
            Element::Link(link) => {
                buffer.push_str("<a href=\"");
                push_escaped_html_attr(buffer, &link.href);
                buffer.push('"');
                if let Some(lang) = &link.lang {
                    buffer.push_str(" lang=\"");
                    push_escaped_html_attr(buffer, lang);
                    buffer.push('"');
                }
                buffer.push('>');
                if let Some(content) = &link.content {
                    Self::render_content_match_type_html(content, buffer);
                }
                buffer.push_str("</a>");
            }
            Element::Styled(styled) => {
                let tag_str = styled.tag.as_html_str();
                buffer.push('<');
                buffer.push_str(tag_str);
                if styled.open == Some(true) {
                    buffer.push_str(" open");
                }
                if let Some(title) = &styled.title {
                    buffer.push_str(" title=\"");
                    push_escaped_html_attr(buffer, title);
                    buffer.push('"');
                }
                if let Some(lang) = &styled.lang {
                    buffer.push_str(" lang=\"");
                    push_escaped_html_attr(buffer, lang);
                    buffer.push('"');
                }
                push_data_attributes(buffer, &styled.data);
                if let Some(style) = &styled.style {
                    style.push_style_attr(buffer);
                }
                buffer.push('>');
                if let Some(content) = &styled.content {
                    Self::render_content_match_type_html(content, buffer);
                }
                buffer.push_str("</");
                buffer.push_str(tag_str);
                buffer.push('>');
            }
            Element::Unstyled(unstyled) => {
                let tag_str = unstyled.tag.as_html_str();
                buffer.push('<');
                buffer.push_str(tag_str);
                if let Some(lang) = &unstyled.lang {
                    buffer.push_str(" lang=\"");
                    push_escaped_html_attr(buffer, lang);
                    buffer.push('"');
                }
                push_data_attributes(buffer, &unstyled.data);
                buffer.push('>');
                if let Some(content) = &unstyled.content {
                    Self::render_content_match_type_html(content, buffer);
                }
                buffer.push_str("</");
                buffer.push_str(tag_str);
                buffer.push('>');
            }
            Element::Table(table) => {
                let tag_str = table.tag.as_html_str();
                buffer.push('<');
                buffer.push_str(tag_str);
                if let Some(col_span) = table.col_span {
                    use std::fmt::Write;
                    let _ = write!(buffer, " colspan=\"{}\"", col_span);
                }
                if let Some(row_span) = table.row_span {
                    use std::fmt::Write;
                    let _ = write!(buffer, " rowspan=\"{}\"", row_span);
                }
                if let Some(lang) = &table.lang {
                    buffer.push_str(" lang=\"");
                    push_escaped_html_attr(buffer, lang);
                    buffer.push('"');
                }
                push_data_attributes(buffer, &table.data);
                if let Some(style) = &table.style {
                    style.push_style_attr(buffer);
                }
                buffer.push('>');
                if let Some(content) = &table.content {
                    Self::render_content_match_type_html(content, buffer);
                }
                buffer.push_str("</");
                buffer.push_str(tag_str);
                buffer.push('>');
            }
            Element::Image(img) => {
                Self::render_image_html(img, buffer);
            }
        }
    }

    fn render_image_html(img: &ImageElement, buffer: &mut CompactString) {
        use std::fmt::Write;
        buffer.push_str("<img src=\"");
        push_escaped_html_attr(buffer, &img.path);
        buffer.push('"');
        if let Some(alt) = &img.alt {
            buffer.push_str(" alt=\"");
            push_escaped_html_attr(buffer, alt);
            buffer.push('"');
        }
        if let Some(title) = &img.title {
            buffer.push_str(" title=\"");
            push_escaped_html_attr(buffer, title);
            buffer.push('"');
        }
        push_data_attributes(buffer, &img.data);

        let unit = match img.size_units {
            Some(SizeUnits::Em) => "em",
            Some(SizeUnits::Px) | None => "px",
        };
        let has_style = img.width.is_some()
            || img.height.is_some()
            || img.vertical_align.is_some()
            || img.border.is_some()
            || img.border_radius.is_some()
            || img.image_rendering.is_some()
            || img.pixelated == Some(true);

        if has_style {
            buffer.push_str(" style=\"");
            if let Some(w) = img.width {
                let _ = write!(buffer, "width:{}{};", w, unit);
            }
            if let Some(h) = img.height {
                let _ = write!(buffer, "height:{}{};", h, unit);
            }
            if let Some(va) = img.vertical_align {
                buffer.push_str("vertical-align:");
                buffer.push_str(va.as_css_str());
                buffer.push(';');
            }
            if let Some(border) = &img.border {
                buffer.push_str("border:");
                push_escaped_html_attr(buffer, border);
                buffer.push(';');
            }
            if let Some(br) = &img.border_radius {
                buffer.push_str("border-radius:");
                push_escaped_html_attr(buffer, br);
                buffer.push(';');
            }
            if let Some(ir) = img.image_rendering {
                buffer.push_str("image-rendering:");
                buffer.push_str(ir.as_css_str());
                buffer.push(';');
            } else if img.pixelated == Some(true) {
                buffer.push_str("image-rendering:pixelated;");
            }
            buffer.push('"');
        }
        buffer.push('>');
    }

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
                    buffer.push_str(&CompactString::from(format!(
                        "[Image: {}]",
                        image_element.path
                    )));
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
    /// A table header group tag.
    #[serde(rename = "thead")]
    Thead,
    /// A table body tag.
    #[serde(rename = "tb", alias = "tbody")]
    TableBody,
    /// A table footer tag.
    #[serde(rename = "tf", alias = "tfoot")]
    TableFooter,
    /// A table row tag.
    #[serde(rename = "tr")]
    TableRow,
    /// A table data tag.
    #[serde(rename = "td")]
    TableData,
    /// A table header tag.
    #[serde(rename = "th")]
    TableHeader,
    /// An unknown table-related tag often found in dictionaries.
    #[serde(rename = "t")]
    TableGeneric,
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

impl ImageRendering {
    #[inline]
    pub const fn as_css_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Pixelated => "pixelated",
            Self::CrispEdges => "crisp-edges",
        }
    }
}

impl HtmlTag {
    #[inline]
    pub const fn as_html_str(self) -> &'static str {
        match self {
            Self::Ruby => "ruby",
            Self::RubyText => "rt",
            Self::RubyParenthesis => "rp",
            Self::Table | Self::TableGeneric => "table",
            Self::Thead => "thead",
            Self::TableBody => "tbody",
            Self::TableFooter => "tfoot",
            Self::TableRow => "tr",
            Self::TableData => "td",
            Self::TableHeader => "th",
            Self::Anchor => "a",
            Self::Span => "span",
            Self::Div => "div",
            Self::OrderedList => "ol",
            Self::UnorderedList => "ul",
            Self::ListItem => "li",
            Self::Details => "details",
            Self::Summary => "summary",
            Self::Break => "br",
            Self::Img => "img",
        }
    }
}

impl VerticalAlign {
    #[inline]
    pub const fn as_css_str(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Sub => "sub",
            Self::Super => "super",
            Self::TextTop => "text-top",
            Self::TextBottom => "text-bottom",
            Self::Middle => "middle",
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }
}

impl TextDecorationLine {
    #[inline]
    pub const fn as_css_str(self) -> &'static str {
        match self {
            Self::Underline => "underline",
            Self::Overline => "overline",
            Self::LineThrough => "line-through",
        }
    }
}

impl TextDecorationLineOrNone {
    #[inline]
    pub const fn as_css_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::TextDecorationLine(line) => line.as_css_str(),
        }
    }
}

impl TextDecorationStyle {
    #[inline]
    pub const fn as_css_str(&self) -> &'static str {
        match self {
            Self::Solid => "solid",
            Self::Double => "double",
            Self::Dotted => "dotted",
            Self::Dashed => "dashed",
            Self::Wavy => "wavy",
        }
    }
}

impl FontStyle {
    #[inline]
    pub const fn as_css_str(&self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Italic => "italic",
        }
    }
}

impl FontWeight {
    #[inline]
    pub const fn as_css_str(&self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Bold => "bold",
        }
    }
}

impl WordBreak {
    #[inline]
    pub const fn as_css_str(&self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::BreakAll => "break-all",
            Self::KeepAll => "keep-all",
        }
    }
}

impl TextAlign {
    #[inline]
    pub const fn as_css_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::End => "end",
            Self::Left => "left",
            Self::Right => "right",
            Self::Center => "center",
            Self::Justify => "justify",
            Self::JustifyAll => "justify-all",
            Self::MatchParent => "match-parent",
        }
    }
}

impl NumberOrString {
    #[inline]
    fn push_css_length(&self, buffer: &mut CompactString) {
        use std::fmt::Write;
        match self {
            Self::Number(n) => {
                if *n == 0.0 {
                    buffer.push('0');
                } else {
                    let _ = write!(buffer, "{}em", n);
                }
            }
            Self::String(s) => {
                push_escaped_html_attr(buffer, s);
            }
        }
    }
}

impl StructuredContentStyle {
    /// Appends ` style="..."` to `buffer` if any style property is set.
    pub fn push_style_attr(&self, buffer: &mut CompactString) {
        let start_len = buffer.len();
        buffer.push_str(" style=\"");
        let attr_prefix_len = buffer.len();

        #[inline]
        fn push_prop(buffer: &mut CompactString, key: &str, val: &str) {
            buffer.push_str(key);
            buffer.push(':');
            push_escaped_html_attr(buffer, val);
            buffer.push(';');
        }

        #[inline]
        fn push_len_prop(buffer: &mut CompactString, key: &str, val: &NumberOrString) {
            buffer.push_str(key);
            buffer.push(':');
            val.push_css_length(buffer);
            buffer.push(';');
        }

        if let Some(v) = &self.font_style {
            push_prop(buffer, "font-style", v.as_css_str());
        }
        if let Some(v) = &self.font_weight {
            push_prop(buffer, "font-weight", v.as_css_str());
        }
        if let Some(v) = &self.font_size {
            push_prop(buffer, "font-size", v);
        }
        if let Some(v) = &self.color {
            push_prop(buffer, "color", v);
        }
        if let Some(v) = &self.background {
            push_prop(buffer, "background", v);
        }
        if let Some(v) = &self.background_color {
            push_prop(buffer, "background-color", v);
        }
        if let Some(v) = &self.text_decoration_line {
            push_prop(buffer, "text-decoration-line", v.as_css_str());
        }
        if let Some(v) = &self.text_decoration_style {
            push_prop(buffer, "text-decoration-style", v.as_css_str());
        }
        if let Some(v) = &self.text_decoration_color {
            push_prop(buffer, "text-decoration-color", v);
        }
        if let Some(v) = &self.border_color {
            push_prop(buffer, "border-color", v);
        }
        if let Some(v) = &self.border_style {
            push_prop(buffer, "border-style", v);
        }
        if let Some(v) = &self.border_radius {
            push_prop(buffer, "border-radius", v);
        }
        if let Some(v) = &self.border_width {
            push_prop(buffer, "border-width", v);
        }
        if let Some(v) = &self.clip_path {
            push_prop(buffer, "clip-path", v);
        }
        if let Some(v) = &self.vertical_align {
            push_prop(buffer, "vertical-align", v.as_css_str());
        }
        if let Some(v) = &self.text_align {
            push_prop(buffer, "text-align", v.as_css_str());
        }
        if let Some(v) = &self.text_emphasis {
            push_prop(buffer, "text-emphasis", v);
        }
        if let Some(v) = &self.text_shadow {
            push_prop(buffer, "text-shadow", v);
        }
        if let Some(v) = &self.margin {
            push_len_prop(buffer, "margin", v);
        }
        if let Some(v) = &self.margin_top {
            push_len_prop(buffer, "margin-top", v);
        }
        if let Some(v) = &self.margin_left {
            push_len_prop(buffer, "margin-left", v);
        }
        if let Some(v) = &self.margin_right {
            push_len_prop(buffer, "margin-right", v);
        }
        if let Some(v) = &self.margin_bottom {
            push_len_prop(buffer, "margin-bottom", v);
        }
        if let Some(v) = &self.padding {
            push_len_prop(buffer, "padding", v);
        }
        if let Some(v) = &self.padding_top {
            push_len_prop(buffer, "padding-top", v);
        }
        if let Some(v) = &self.padding_left {
            push_len_prop(buffer, "padding-left", v);
        }
        if let Some(v) = &self.padding_right {
            push_len_prop(buffer, "padding-right", v);
        }
        if let Some(v) = &self.padding_bottom {
            push_len_prop(buffer, "padding-bottom", v);
        }
        if let Some(v) = &self.word_break {
            push_prop(buffer, "word-break", v.as_css_str());
        }
        if let Some(v) = &self.white_space {
            push_prop(buffer, "white-space", v);
        }
        if let Some(v) = &self.cursor {
            push_prop(buffer, "cursor", v);
        }
        if let Some(v) = &self.list_style_type {
            push_prop(buffer, "list-style-type", v);
        }

        if buffer.len() == attr_prefix_len {
            buffer.truncate(start_len);
        } else {
            buffer.push('"');
        }
    }
}
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
            "div" | "span" | "ol" | "ul" | "li" | "details" | "summary" => Ok(Element::Styled(
                de::Deserialize::deserialize(de::value::SeqAccessDeserializer::new(seq))?,
            )),
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
        if let Some(key) = map.next_key::<CompactString>()? {
            if key == "tag" {
                let tag: HtmlTag = map.next_value()?;
                return Element::deserialize_from_map(tag, map);
            }
            // Not "tag", buffer.
            let mut buffered = IndexMap::new();
            buffered.insert(key, map.next_value::<Value>()?);
            while let Some(k) = map.next_key::<CompactString>()? {
                if k == "tag" {
                    let tag: HtmlTag = map.next_value()?;
                    // Consume the rest of the map into the buffered map.
                    while let Some(k2) = map.next_key::<CompactString>()? {
                        buffered.insert(k2, map.next_value()?);
                    }
                    return Element::deserialize_from_buffered_map(tag, buffered)
                        .map_err(de::Error::custom);
                }
                buffered.insert(k, map.next_value()?);
            }
            return Err(de::Error::missing_field("tag"));
        }
        Err(de::Error::missing_field("tag"))
    }
}

impl Element {
    /// Deserializes an element from a map that has already had its "tag" field consumed.
    pub fn deserialize_from_map<'de, A>(tag: HtmlTag, map: A) -> Result<Self, A::Error>
    where
        A: MapAccess<'de>,
    {
        match tag {
            HtmlTag::Anchor => Ok(Element::Link(LinkElement::deserialize_from_map(tag, map)?)),
            HtmlTag::Div
            | HtmlTag::Span
            | HtmlTag::OrderedList
            | HtmlTag::UnorderedList
            | HtmlTag::ListItem
            | HtmlTag::Details
            | HtmlTag::Summary => Ok(Element::Styled(StyledElement::deserialize_from_map(
                tag, map,
            )?)),
            HtmlTag::Ruby
            | HtmlTag::RubyText
            | HtmlTag::RubyParenthesis
            | HtmlTag::Table
            | HtmlTag::Thead
            | HtmlTag::TableFooter
            | HtmlTag::TableBody
            | HtmlTag::TableRow
            | HtmlTag::TableGeneric => Ok(Element::Unstyled(
                UnstyledElement::deserialize_from_map(tag, map)?,
            )),
            HtmlTag::TableData | HtmlTag::TableHeader => Ok(Element::Table(
                TableElement::deserialize_from_map(tag, map)?,
            )),
            HtmlTag::Break => Ok(Element::LineBreak(LineBreak::deserialize_from_map(
                tag, map,
            )?)),
            HtmlTag::Img => Ok(Element::Image(ImageElement::deserialize_from_map(
                tag, map,
            )?)),
        }
    }

    pub fn deserialize_from_buffered_map(
        tag: HtmlTag,
        fields: IndexMap<CompactString, Value>,
    ) -> Result<Self, String> {
        match tag {
            HtmlTag::Anchor => Ok(Element::Link(LinkElement::deserialize_from_value_map(
                tag, fields,
            )?)),
            HtmlTag::Div
            | HtmlTag::Span
            | HtmlTag::OrderedList
            | HtmlTag::UnorderedList
            | HtmlTag::ListItem
            | HtmlTag::Details
            | HtmlTag::Summary => Ok(Element::Styled(StyledElement::deserialize_from_value_map(
                tag, fields,
            )?)),
            HtmlTag::Ruby
            | HtmlTag::RubyText
            | HtmlTag::RubyParenthesis
            | HtmlTag::Table
            | HtmlTag::Thead
            | HtmlTag::TableFooter
            | HtmlTag::TableBody
            | HtmlTag::TableRow
            | HtmlTag::TableGeneric => Ok(Element::Unstyled(
                UnstyledElement::deserialize_from_value_map(tag, fields)?,
            )),
            HtmlTag::TableData | HtmlTag::TableHeader => Ok(Element::Table(
                TableElement::deserialize_from_value_map(tag, fields)?,
            )),
            HtmlTag::Break => Ok(Element::LineBreak(LineBreak::deserialize_from_value_map(
                tag, fields,
            )?)),
            HtmlTag::Img => Ok(Element::Image(ImageElement::deserialize_from_value_map(
                tag, fields,
            )?)),
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
    pub data: Option<IndexMap<CompactString, CompactString>>,
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
    pub data: Option<IndexMap<CompactString, CompactString>>,
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
                        if let Ok(s) =
                            serde_json::from_value::<StructuredContentStyle>(value.clone())
                        {
                            style = Some(s);
                            continue;
                        }

                        // Try to see if it's a data object
                        if let Ok(d) = serde_json::from_value::<
                            IndexMap<CompactString, CompactString>,
                        >(value.clone())
                        {
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
                    data: helper
                        .data
                        .map(|m| m.into_iter().map(|(k, v)| (k.into(), v.into())).collect()),
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
    pub data: Option<IndexMap<CompactString, CompactString>>,
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
    pub lang: Option<CompactString>,
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
    pub data: Option<IndexMap<CompactString, CompactString>>,
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
    pub fn deserialize_from_map<'de, A>(tag: HtmlTag, mut map: A) -> Result<Self, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut content = None;
        let mut href = None;
        let mut lang = None;
        while let Some(key) = map.next_key::<CompactString>()? {
            match key.as_str() {
                "tag" => {
                    let _: HtmlTag = map.next_value()?;
                }
                "content" => content = Some(map.next_value()?),
                "href" => href = Some(map.next_value()?),
                "lang" => lang = Some(map.next_value()?),
                _ => {
                    let _: de::IgnoredAny = map.next_value()?;
                }
            }
        }
        let href = href.ok_or_else(|| de::Error::missing_field("href"))?;
        Ok(Self {
            tag,
            content,
            href,
            lang,
        })
    }

    pub fn deserialize_from_value_map(
        tag: HtmlTag,
        mut fields: IndexMap<CompactString, Value>,
    ) -> Result<Self, String> {
        let content = fields
            .swap_remove("content")
            .map(serde_json::from_value::<ContentMatchType>)
            .transpose()
            .map_err(|e| e.to_string())?;
        let href = fields
            .swap_remove("href")
            .map(serde_json::from_value::<CompactString>)
            .transpose()
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "missing field href".to_string())?;
        let lang = fields
            .swap_remove("lang")
            .map(serde_json::from_value::<CompactString>)
            .transpose()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            tag,
            content,
            href,
            lang,
        })
    }

    pub fn deserialize_with_tag(tag: String, mut value: Value) -> Result<Self, String> {
        if let Some(obj) = value.as_object_mut() {
            obj.insert("tag".to_string(), Value::String(tag));
        }
        serde_json::from_value(value).map_err(|e| e.to_string())
    }
}

impl StyledElement {
    pub fn deserialize_from_map<'de, A>(tag: HtmlTag, mut map: A) -> Result<Self, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut content = None;
        let mut data = None;
        let mut style = None;
        let mut title = None;
        let mut open = None;
        let mut lang = None;
        while let Some(key) = map.next_key::<CompactString>()? {
            match key.as_str() {
                "tag" => {
                    let _: HtmlTag = map.next_value()?;
                }
                "content" => content = Some(map.next_value()?),
                "data" => data = Some(map.next_value()?),
                "style" => style = Some(map.next_value()?),
                "title" => title = Some(map.next_value()?),
                "open" => open = Some(map.next_value()?),
                "lang" => lang = Some(map.next_value()?),
                _ => {
                    let _: de::IgnoredAny = map.next_value()?;
                }
            }
        }
        Ok(Self {
            tag,
            content,
            data,
            style,
            title,
            open,
            lang,
        })
    }

    pub fn deserialize_from_value_map(
        tag: HtmlTag,
        mut fields: IndexMap<CompactString, Value>,
    ) -> Result<Self, String> {
        let content = fields
            .swap_remove("content")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?;
        let data = fields
            .swap_remove("data")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?;
        let style = fields
            .swap_remove("style")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?;
        let title = fields
            .swap_remove("title")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?;
        let open = fields
            .swap_remove("open")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?;
        let lang = fields
            .swap_remove("lang")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            tag,
            content,
            data,
            style,
            title,
            open,
            lang,
        })
    }

    pub fn deserialize_with_tag(tag: String, mut value: Value) -> Result<Self, String> {
        if let Some(obj) = value.as_object_mut() {
            obj.insert("tag".to_string(), Value::String(tag));
        }
        serde_json::from_value(value).map_err(|e| e.to_string())
    }
}

impl UnstyledElement {
    pub fn deserialize_from_map<'de, A>(tag: HtmlTag, mut map: A) -> Result<Self, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut content = None;
        let mut data = None;
        let mut lang = None;
        while let Some(key) = map.next_key::<CompactString>()? {
            match key.as_str() {
                "tag" => {
                    let _: HtmlTag = map.next_value()?;
                }
                "content" => content = Some(map.next_value()?),
                "data" => data = Some(map.next_value()?),
                "lang" => lang = Some(map.next_value()?),
                _ => {
                    let _: de::IgnoredAny = map.next_value()?;
                }
            }
        }
        Ok(Self {
            tag,
            content,
            data,
            lang,
        })
    }

    pub fn deserialize_from_value_map(
        tag: HtmlTag,
        mut fields: IndexMap<CompactString, Value>,
    ) -> Result<Self, String> {
        let content = fields
            .swap_remove("content")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?;
        let data = fields
            .swap_remove("data")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?;
        let lang = fields
            .swap_remove("lang")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            tag,
            content,
            data,
            lang,
        })
    }

    pub fn deserialize_with_tag(tag: String, mut value: Value) -> Result<Self, String> {
        if let Some(obj) = value.as_object_mut() {
            obj.insert("tag".to_string(), Value::String(tag));
        }
        serde_json::from_value(value).map_err(|e| e.to_string())
    }
}

impl TableElement {
    pub fn deserialize_from_map<'de, A>(tag: HtmlTag, mut map: A) -> Result<Self, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut content = None;
        let mut data = None;
        let mut col_span = None;
        let mut row_span = None;
        let mut style = None;
        let mut lang = None;
        while let Some(key) = map.next_key::<CompactString>()? {
            match key.as_str() {
                "tag" => {
                    let _: HtmlTag = map.next_value()?;
                }
                "content" => content = Some(map.next_value()?),
                "data" => data = Some(map.next_value()?),
                "colSpan" => col_span = Some(map.next_value()?),
                "rowSpan" => row_span = Some(map.next_value()?),
                "style" => style = Some(map.next_value()?),
                "lang" => lang = Some(map.next_value()?),
                _ => {
                    let _: de::IgnoredAny = map.next_value()?;
                }
            }
        }
        Ok(Self {
            tag,
            content,
            data,
            col_span,
            row_span,
            style,
            lang,
        })
    }

    pub fn deserialize_from_value_map(
        tag: HtmlTag,
        mut fields: IndexMap<CompactString, Value>,
    ) -> Result<Self, String> {
        let content = fields
            .swap_remove("content")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?;
        let data = fields
            .swap_remove("data")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?;
        let col_span = fields
            .swap_remove("colSpan")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?;
        let row_span = fields
            .swap_remove("rowSpan")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?;
        let style = fields
            .swap_remove("style")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?;
        let lang = fields
            .swap_remove("lang")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            tag,
            content,
            data,
            col_span,
            row_span,
            style,
            lang,
        })
    }

    pub fn deserialize_with_tag(tag: String, mut value: Value) -> Result<Self, String> {
        if let Some(obj) = value.as_object_mut() {
            obj.insert("tag".to_string(), Value::String(tag));
        }
        serde_json::from_value(value).map_err(|e| e.to_string())
    }
}

impl LineBreak {
    pub fn deserialize_from_map<'de, A>(tag: HtmlTag, mut map: A) -> Result<Self, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut data = None;
        while let Some(key) = map.next_key::<CompactString>()? {
            match key.as_str() {
                "tag" => {
                    let _: HtmlTag = map.next_value()?;
                }
                "data" => data = Some(map.next_value()?),
                _ => {
                    let _: de::IgnoredAny = map.next_value()?;
                }
            }
        }
        Ok(Self { tag, data })
    }

    pub fn deserialize_from_value_map(
        tag: HtmlTag,
        mut fields: IndexMap<CompactString, Value>,
    ) -> Result<Self, String> {
        let data = fields
            .swap_remove("data")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?;
        Ok(Self { tag, data })
    }

    pub fn deserialize_with_tag(tag: String, mut value: Value) -> Result<Self, String> {
        if let Some(obj) = value.as_object_mut() {
            obj.insert("tag".to_string(), Value::String(tag));
        }
        serde_json::from_value(value).map_err(|e| e.to_string())
    }
}

impl ImageElement {
    pub fn deserialize_from_map<'de, A>(tag: HtmlTag, mut map: A) -> Result<Self, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut path = None;
        let mut width = None;
        let mut height = None;
        let mut title = None;
        let mut alt = None;
        let mut description = None;
        let mut pixelated = None;
        let mut image_rendering = None;
        let mut appearance = None;
        let mut background = None;
        let mut collapsed = None;
        let mut collapsible = None;
        let mut data = None;
        let mut content = None;
        let mut vertical_align = None;
        let mut border = None;
        let mut border_radius = None;
        let mut size_units = None;

        while let Some(key) = map.next_key::<CompactString>()? {
            match key.as_str() {
                "tag" => {
                    let _: HtmlTag = map.next_value()?;
                }
                "path" => path = Some(map.next_value()?),
                "width" => width = Some(map.next_value()?),
                "height" => height = Some(map.next_value()?),
                "title" => title = Some(map.next_value()?),
                "alt" => alt = Some(map.next_value()?),
                "description" => description = Some(map.next_value()?),
                "pixelated" => pixelated = Some(map.next_value()?),
                "imageRendering" => image_rendering = Some(map.next_value()?),
                "appearance" => appearance = Some(map.next_value()?),
                "background" => background = Some(map.next_value()?),
                "collapsed" => collapsed = Some(map.next_value()?),
                "collapsible" => collapsible = Some(map.next_value()?),
                "data" => data = Some(map.next_value()?),
                "content" => content = Some(map.next_value()?),
                "verticalAlign" => vertical_align = Some(map.next_value()?),
                "border" => border = Some(map.next_value()?),
                "borderRadius" => border_radius = Some(map.next_value()?),
                "sizeUnits" => size_units = Some(map.next_value()?),
                _ => {
                    let _: de::IgnoredAny = map.next_value()?;
                }
            }
        }
        let path = path.ok_or_else(|| de::Error::missing_field("path"))?;
        Ok(Self {
            tag,
            path,
            width,
            height,
            preferred_width: None,
            preferred_height: None,
            title,
            alt,
            description,
            pixelated,
            image_rendering,
            appearance,
            background,
            collapsed,
            collapsible,
            data,
            content,
            vertical_align,
            border,
            border_radius,
            size_units,
        })
    }

    pub fn deserialize_from_value_map(
        tag: HtmlTag,
        mut fields: IndexMap<CompactString, Value>,
    ) -> Result<Self, String> {
        let path = fields
            .swap_remove("path")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "missing field path".to_string())?;
        let width = fields
            .swap_remove("width")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?;
        let height = fields
            .swap_remove("height")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?;
        let title = fields
            .swap_remove("title")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?;
        let alt = fields
            .swap_remove("alt")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?;
        let description = fields
            .swap_remove("description")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?;
        let pixelated = fields
            .swap_remove("pixelated")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?;
        let image_rendering = fields
            .swap_remove("imageRendering")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?;
        let appearance = fields
            .swap_remove("appearance")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?;
        let background = fields
            .swap_remove("background")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?;
        let collapsed = fields
            .swap_remove("collapsed")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?;
        let collapsible = fields
            .swap_remove("collapsible")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?;
        let data = fields
            .swap_remove("data")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?;
        let content = fields
            .swap_remove("content")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?;
        let vertical_align = fields
            .swap_remove("verticalAlign")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?;
        let border = fields
            .swap_remove("border")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?;
        let border_radius = fields
            .swap_remove("borderRadius")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?;
        let size_units = fields
            .swap_remove("sizeUnits")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?;

        Ok(Self {
            tag,
            path,
            width,
            height,
            preferred_width: None,
            preferred_height: None,
            title,
            alt,
            description,
            pixelated,
            image_rendering,
            appearance,
            background,
            collapsed,
            collapsible,
            data,
            content,
            vertical_align,
            border,
            border_radius,
            size_units,
        })
    }

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
                    data: helper
                        .data
                        .map(|m| m.into_iter().map(|(k, v)| (k.into(), v.into())).collect()),
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
                        if let Ok(d) = serde_json::from_value::<
                            IndexMap<CompactString, CompactString>,
                        >(value.clone())
                        {
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
                    data: helper
                        .data
                        .map(|m| m.into_iter().map(|(k, v)| (k.into(), v.into())).collect()),
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
                        if let Ok(d) = serde_json::from_value::<
                            IndexMap<CompactString, CompactString>,
                        >(value.clone())
                        {
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
                    data: helper
                        .data
                        .map(|m| m.into_iter().map(|(k, v)| (k.into(), v.into())).collect()),
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
                    data: helper
                        .data
                        .map(|m| m.into_iter().map(|(k, v)| (k.into(), v.into())).collect()),
                })
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let tag: HtmlTag = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let data: Option<IndexMap<CompactString, CompactString>> =
                    seq.next_element()?.unwrap_or(None);

                Ok(LineBreak { tag, data })
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

    #[test]
    fn test_html_rendering_and_plain_text_stripping() {
        let json_sc = json!({
            "type": "structured-content",
            "content": [
                {
                    "tag": "div",
                    "data": { "content": "header" },
                    "style": { "fontWeight": "bold", "color": "#ff0000", "marginLeft": 0.5 },
                    "content": "見出し <1> & \"quote\""
                },
                {
                    "tag": "ul",
                    "content": [
                        { "tag": "li", "content": "項目A" },
                        { "tag": "li", "content": ["項目B: ", { "tag": "a", "href": "?query=test&mode=exact", "content": "リンク" }] }
                    ]
                },
                {
                    "tag": "table",
                    "content": [
                        {
                            "tag": "tr",
                            "content": [
                                { "tag": "th", "colSpan": 2, "content": "表題" }
                            ]
                        },
                        {
                            "tag": "tr",
                            "content": [
                                { "tag": "td", "content": "セル1" },
                                { "tag": "td", "content": "セル2" }
                            ]
                        }
                    ]
                },
                {
                    "tag": "img",
                    "path": "images/diagram.png",
                    "width": 100.0,
                    "height": 50.0,
                    "sizeUnits": "px"
                }
            ]
        });

        let glossary_content: TermGlossaryContent = serde_json::from_value(json_sc).unwrap();
        let expected_plain = glossary_content.to_plain_text();
        let group = TermGlossaryContentGroup::from(glossary_content);

        assert_eq!(
            group.as_html(),
            "<div data-sc-content=\"header\" style=\"font-weight:bold;color:#ff0000;margin-left:0.5em;\">見出し &lt;1&gt; &amp; \"quote\"</div>\
             <ul><li>項目A</li><li>項目B: <a href=\"?query=test&amp;mode=exact\">リンク</a></li></ul>\
             <table><tr><th colspan=\"2\">表題</th></tr><tr><td>セル1</td><td>セル2</td></tr></table>\
             <img src=\"images/diagram.png\" style=\"width:100px;height:50px;\">"
        );

        let stripped_plain = group.to_plain_text();
        assert_eq!(stripped_plain, expected_plain);
    }

    #[test]
    fn test_plain_string_glossary_html_and_roundtrip() {
        let raw = TermGlossaryContent::String("行1 <test> & more\n行2".into());
        let expected_plain = raw.to_plain_text();
        let group = TermGlossaryContentGroup::from(raw);
        assert_eq!(group.as_html(), "行1 &lt;test&gt; &amp; more<br>行2");
        assert_eq!(group.to_plain_text(), expected_plain);
    }
}
