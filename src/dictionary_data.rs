//! Contains the data structures for the Yomichan dictionary format.

use indexmap::IndexMap;

use serde::{Deserialize, Deserializer, Serialize};
use std::string::String;

use crate::dictionary_database::TermMetaPhoneticData;
use crate::dictionary_importer::FrequencyMode;
use crate::errors::{DictionaryFileError, ImportError};
use crate::structured_content::ImageElement;
use crate::utils::_convert_string_to_number;

trait StrMacro {
    fn from_static_str(s: &'static ::core::primitive::str) -> Self;
}
impl StrMacro for &::core::primitive::str {
    fn from_static_str(s: &'static ::core::primitive::str) -> Self {
        s
    }
}
impl StrMacro for ::std::string::String {
    fn from_static_str(s: &'static ::core::primitive::str) -> Self {
        ::std::borrow::ToOwned::to_owned(s)
    }
}
macro_rules! str {
    ($s:literal) => {
        StrMacro::from_static_str($s)
    };
}

// #[rustfmt::skip]
// pub static KANA_MAP: LazyLock<BiHashMap<&'static str, &'static str>> = LazyLock::new(|| {
//     BiHashMap::from_iter([
//         ("ア", "あ"), ("イ", "い"), ("ウ", "う"), ("エ", "え"), ("オ", "お"),
//         ("カ", "か"), ("キ", "き"), ("ク", "く"), ("ケ", "け"), ("コ", "こ"),
//         ("サ", "さ"), ("シ", "し"), ("ス", "す"), ("セ", "せ"), ("ソ", "そ"),
//         ("タ", "た"), ("チ", "ち"), ("ツ", "つ"), ("テ", "て"), ("ト", "と"),
//         ("ナ", "な"), ("ニ", "に"), ("ヌ", "ぬ"), ("ネ", "ね"), ("ノ", "の"),
//         ("ハ", "は"), ("ヒ", "ひ"), ("フ", "ふ"), ("ヘ", "へ"), ("ホ", "ほ"),
//         ("マ", "ま"), ("ミ", "み"), ("ム", "む"), ("メ", "め"), ("モ", "も"),
//         ("ヤ", "や"), ("ユ", "ゆ"), ("ヨ", "よ"), ("ラ", "ら"), ("リ", "り"),
//         ("ル", "る"), ("レ", "れ"), ("ロ", "ろ"), ("ワ", "わ"), ("ヲ", "を"),
//         ("ン", "ん"), ("ガ", "が"), ("ギ", "ぎ"), ("グ", "ぐ"), ("ゲ", "げ"),
//         ("ゴ", "ご"), ("ザ", "ざ"), ("ジ", "じ"), ("ズ", "ず"), ("ゼ", "ぜ"),
//         ("ゾ", "ぞ"), ("ダ", "だ"), ("ヂ", "ぢ"), ("ヅ", "づ"), ("デ", "で"),
//         ("ド", "ど"), ("バ", "ば"), ("ビ", "び"), ("ブ", "ぶ"), ("ベ", "べ"),
//         ("ボ", "ぼ"), ("パ", "ぱ"), ("ピ", "ぴ"), ("プ", "ぷ"), ("ペ", "ぺ"),
//         ("ポ", "ぽ"),   ("キャ", "きゃ"), ("キュ", "きゅ"), ("キョ", "きょ"),
//         ("シャ", "しゃ"), ("シュ", "しゅ"), ("ショ", "しょ"), ("チャ", "ちゃ"),
//         ("チュ", "ちゅ"), ("チョ", "ちょ"), ("ニャ", "にゃ"), ("ニュ", "にゅ"),
//         ("ニョ", "にょ"), ("ヒャ", "ひゃ"), ("ヒュ", "ひゅ"), ("ヒョ", "ひょ"),
//         ("ミャ", "みゃ"), ("ミュ", "みゅ"), ("ミョ", "みょ"), ("リャ", "りゃ"),
//         ("リュ", "りゅ"), ("リョ", "りょ"),  ("ギャ", "ぎゃ"), ("ギュ", "ぎゅ"),
//         ("ギョ", "ぎょ"), ("ジャ", "じゃ"), ("ジュ", "じゅ"), ("ジョ", "じょ"),
//         ("ビャ", "びゃ"), ("ビュ", "びゅ"), ("ビョ", "びょ"), ("ピャ", "ぴゃ"),
//         ("ピュ", "ぴゅ"), ("ピョ", "ぴょ"),
//     ])
// });
//
// #[derive(Serialize, Deserialize, Debug)]
// pub struct TermEntry {
//     pub dictionary: String,
//     pub expression: String,
//     pub reading: String,
//     pub sequence: Option<String>,
// }

/// The type of glossary entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TermGlossaryType {
    /// The glossary entry is text.
    Text,
    /// The glossary entry is an image.
    Image,
}

/// An image in a glossary entry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TermGlossaryImage {
    /// The type of the glossary entry.
    pub term_glossary_type: TermGlossaryType,
    /// The image element.
    pub term_image: Option<ImageElement>,
}

/// Represents the metadata of a dictionary.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct YomichanIndexFile {
    /// Title of the dictionary.
    pub title: String,
    /// Revision of the dictionary.
    ///
    /// This value is only used for displaying information.
    pub revision: String,
    /// Whether or not this dictionary contains sequencing information for related terms.
    pub sequenced: Option<bool>,
    /// Format of data found in the JSON data files.
    pub format: Option<u8>,
    /// Alias for format.
    /// Versions can include: `1 - 3`.
    pub version: Option<u8>,
    /// The minimum version of Yomitan required to use this dictionary.
    pub minimum_yomitan_version: Option<String>,
    /// Whether this dictionary can be updated.
    pub is_updatable: Option<bool>,
    /// The URL where the index file can be found.
    pub index_url: Option<String>,
    /// The URL where the dictionary can be downloaded.
    pub download_url: Option<String>,
    /// Creator of the dictionary.
    pub author: Option<String>,
    /// URL for the source of the dictionary.
    pub url: Option<String>,
    /// Description of the dictionary data.
    pub description: Option<String>,
    /// Attribution information for the dictionary data.
    pub attribution: Option<String>,
    /// Language of the terms in the dictionary.
    ///
    /// See: [iso639 code list](https://www.loc.gov/standards/iso639-2/php/code_list.php).
    pub source_language: Option<String>,
    /// Main language of the definitions in the dictionary.
    ///
    /// See: [iso639 code list](https://www.loc.gov/standards/iso639-2/php/code_list.php).
    pub target_language: Option<String>,
    /// The frequency mode of the dictionary.
    pub frequency_mode: Option<FrequencyMode>,
    /// The tag metadata of the dictionary.
    pub tag_meta: Option<IndexMap<String, IndexTag>>,
}
impl YomichanIndexFile {
    /// Converts an index file to a `YomichanIndexFile` struct.
    pub fn convert_index_file(
        outpath: std::path::PathBuf,
    ) -> Result<YomichanIndexFile, ImportError> {
        let index_str =
            std::fs::read_to_string(&outpath).map_err(|e| DictionaryFileError::File {
                outpath,
                reason: e.to_string(),
            })?;
        let index: YomichanIndexFile = serde_json::from_str(&index_str)?;
        Ok(index)
    }
}

/// Tag information for terms and kanji.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IndexTagMeta {
    /// A map of tags.
    pub tags: IndexMap<String, IndexTag>,
}

// #[deprecated(since = "0.0.1", note = "individual tag files should be used instead")]
/// Tag information for terms and kanji.
///
/// This object is deprecated, and individual tag files should be used instead.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IndexTag {
    category: String,
    order: u16,
    notes: String,
    score: u16,
}

/// Information about a single tag.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DictionaryDataTag {
    /// Tag name.
    pub name: String,
    /// Category for the tag.
    pub category: String,
    /// Sorting order for the tag.
    pub order: u64,
    /// Notes for the tag.
    pub notes: String,
    /// Score used to determine popularity.
    ///
    /// Negative values are more rare and positive values are more frequent.
    /// This score is also used to sort search results.
    pub score: i128,
}

// #[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
// pub struct TermGlossaryText {
//     pub term_glossary_type: TermGlossaryType,
//     pub text: String,
// }

/// Yomichan-like term model.
///
/// Related: [`TermGlossaryContent`]
// #[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
// pub struct TermV3 {
//     pub expression: String,
//     pub reading: String,
//     pub definition_tags: Option<String>,
//     pub rules: String,
//     pub score: i128,
//     pub glossary: Vec<TermGlossary>,
//     pub sequence: i64,
//     pub term_tags: String,
// }

/// Custom `Yomichan.rs`-unique term model.
// #[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
// pub struct TermV4 {
//     pub expression: String,
//     pub reading: String,
//     pub definition_tags: Option<String>,
//     pub rules: String,
//     pub score: i8,
//     pub definition: String,
//     pub sequence: i128,
//     pub term_tags: String,
// }

/************* Term Meta *************/

/// A term metadata entry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TermMeta {
    /// The term expression.
    pub expression: String,
    /// The type of metadata.
    pub mode: TermMetaModeType,
    /// The metadata content.
    pub data: MetaDataMatchType,
}

/// The metadata of a term.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum MetaDataMatchType {
    /// Frequency metadata.
    Frequency(TermMetaFreqDataMatchType),
    /// Pitch accent metadata.
    Pitch(TermMetaPitchData),
    /// Phonetic transcription metadata.
    Phonetic(TermMetaPhoneticData),
}

impl<'de> Deserialize<'de> for MetaDataMatchType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        serde_untagged::UntaggedEnumVisitor::new()
            .string(|str| {
                Ok(MetaDataMatchType::Frequency(
                    TermMetaFreqDataMatchType::Generic(GenericFreqData::String(str.to_string())),
                ))
            })
            .i128(|int| {
                Ok(MetaDataMatchType::Frequency(
                    TermMetaFreqDataMatchType::Generic(GenericFreqData::Integer(int)),
                ))
            })
            .map(|map| {
                let value = map.deserialize::<serde_json::Value>()?;
                #[allow(clippy::if_same_then_else)]
                if value.get("frequency").is_some() {
                    serde_json::from_value(value)
                        .map(MetaDataMatchType::Frequency)
                        .map_err(serde::de::Error::custom)
                } else if value.get("value").is_some() {
                    serde_json::from_value(value)
                        .map(MetaDataMatchType::Frequency)
                        .map_err(serde::de::Error::custom)
                } else if value.get("pitches").is_some() {
                    serde_json::from_value(value)
                        .map(MetaDataMatchType::Pitch)
                        .map_err(serde::de::Error::custom)
                } else if value.get("transcriptions").is_some() {
                    serde_json::from_value(value)
                        .map(MetaDataMatchType::Phonetic)
                        .map_err(serde::de::Error::custom)
                } else {
                    Err(serde::de::Error::custom(format!(
                        "[yomichan-rs] Unknown term meta data type: {value:?}"
                    )))
                }
            })
            .deserialize(deserializer)
    }
}

/// The main type of [TermMeta] entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TermMetaModeType {
    /// Frequency metadata.
    Freq,
    /// Pitch accent metadata.
    Pitch,
    /// IPA transcription metadata.
    Ipa,
}
impl From<TermMetaModeType> for u8 {
    fn from(value: TermMetaModeType) -> Self {
        match value {
            TermMetaModeType::Freq => 0,
            TermMetaModeType::Pitch => 1,
            TermMetaModeType::Ipa => 2,
        }
    }
}

/************* Frequency *************/

/// The frequency metadata of a term.
///
/// This is currently use to [`Deserialize`] terms from
/// term_meta_bank_$ files.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TermMetaFrequency {
    /// The term expression.
    pub expression: String,
    /// This will be `"freq"` in the json.
    pub mode: TermMetaModeType,
    /// The frequency data.
    pub data: TermMetaFreqDataMatchType,
}

/// Information about the frequency of a term.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FrequencyInfo {
    /// The frequency of the term.
    pub frequency: i128,
    /// The display value of the frequency.
    pub display_value: Option<String>,
    /// Whether the display value is parsed.
    pub display_value_parsed: bool,
}

/// The frequency data of a term.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
//#[serde(untagged)]
pub enum TermMetaFreqDataMatchType {
    /// Frequency data with a reading.
    WithReading(TermMetaFreqDataWithReading),
    /// Generic frequency data.
    Generic(GenericFreqData),
}

/// Generic frequency data.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
//#[serde(untagged)]
pub enum GenericFreqData {
    /// Frequency data as an object.
    Object(FreqObjectData),
    /// Frequency data as an integer.
    Integer(i128),
    /// Frequency data as a string.
    String(String),
}
impl GenericFreqData {
    pub fn get_frequency_info(&self) -> FrequencyInfo {
        match self {
            GenericFreqData::Object(obj) => FrequencyInfo {
                frequency: obj.value,
                display_value: obj.display_value.clone(),
                display_value_parsed: false,
            },
            GenericFreqData::Integer(num) => FrequencyInfo {
                frequency: *num,
                display_value: None,
                display_value_parsed: false,
            },
            GenericFreqData::String(s_val) => {
                let numeric_value = _convert_string_to_number(&s_val);
                FrequencyInfo {
                    frequency: numeric_value,
                    display_value: Some(s_val.clone()),
                    display_value_parsed: true,
                }
            }
        }
    }
}

/// Frequency data as an object.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FreqObjectData {
    /// The frequency value.
    pub value: i128,
    /// The display value of the frequency.
    #[serde(rename = "displayValue")]
    pub display_value: Option<String>,
}

/// Frequency data with a reading.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TermMetaFreqDataWithReading {
    /// The reading of the term.
    pub reading: String,
    /// The frequency data.
    pub frequency: GenericFreqData,
}

impl GenericFreqData {
    /// Tries to get the reading from the frequency data.
    pub fn try_get_reading(&self) -> Option<&String> {
        match self {
            Self::Integer(_) => None,
            Self::String(str) => Some(str),
            Self::Object(obj) => obj.display_value.as_ref(),
            //Self::WithReading(wr) => Some(&wr.reading),
        }
    }
}

/************* Pitch / Speech Data *************/

/// The pitch metadata of a term.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TermMetaPitch {
    expression: String,
    /// This will be `"pitch"` in the json.
    mode: TermMetaModeType,
    data: TermMetaPitchData,
}

// Helper enum to match [TermMetaPitchAccent] data more accurately.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum VecNumOrNum {
    /// A vector of numbers.
    Vec(Vec<u8>),
    /// A single number.
    Num(u8),
}

/// List of different pitch accent information for the term and reading combination.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pitch {
    /// Mora position of the pitch accent downstep.
    /// A value of 0 indicates that the word does not have a downstep (heiban).
    pub position: u8,
    /// Positions of a morae with nasal sound.
    pub nasal: Option<VecNumOrNum>,
    /// Positions of morae with devoiced sound.
    pub devoice: Option<VecNumOrNum>,
    /// List of tags for this pitch accent.
    /// This typically corresponds to a certain type of part of speech.
    pub tags: Option<Vec<String>>,
}

/// The pitch data of a term.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TermMetaPitchData {
    /// The reading of the term.
    pub reading: String,
    /// The pitch accent information.
    pub pitches: Vec<Pitch>,
}

/************* Kanji Data *************/

// #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
// pub struct KanjiMetaFrequency {
//     character: String,
//     mode: TermMetaModeType,
//     data: GenericFreqData,
// }

pub mod dictionary_data_util {
    use fancy_regex::Regex;
    use std::sync::LazyLock;
    use url::{ParseError as UrlParseError, Url};

    pub static SIMPLE_VERSION_TEST: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(\d+\.)*\d+$").unwrap());

    pub fn compare_revisions(current: &str, latest: &str) -> bool {
        // If either string doesn't match the simple version format,
        // fall back to a lexicographical string comparison.
        if !SIMPLE_VERSION_TEST.is_match(current).unwrap()
            || !SIMPLE_VERSION_TEST.is_match(latest).unwrap()
        {
            return current < latest;
        }

        // The regex ensures all parts are digits, so `unwrap()` is safe here.
        let current_parts: Vec<u32> = current
            .split('.')
            .map(|part| part.parse::<u32>().unwrap())
            .collect();

        let latest_parts: Vec<u32> = latest
            .split('.')
            .map(|part| part.parse::<u32>().unwrap())
            .collect();

        // This logic is from the original JS: if the number of parts is
        // different, fall back to a string comparison. This can cause
        // unexpected results (e.g., "1.5" vs "1.20" would be false).
        if current_parts.len() != latest_parts.len() {
            return current < latest;
        }

        // Compare each version part numerically.
        for i in 0..current_parts.len() {
            if current_parts[i] != latest_parts[i] {
                return current_parts[i] < latest_parts[i];
            }
        }
        false
    }

    pub fn validate_url(s: &str) -> Result<(), UrlParseError> {
        let Err(e) = Url::parse(s) else {
            return Ok(());
        };
        Err(e)
    }
}
