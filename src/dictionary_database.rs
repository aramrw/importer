use crate::dictionary_data::{
    MetaDataMatchType, TermMeta, TermMetaFreqDataMatchType, TermMetaModeType, TermMetaPitchData,
};
use crate::dictionary_importer::DictionarySummary;
use crate::errors::DictionaryFileError;
use crate::structured_content::TermGlossaryGroupType;
use indexmap::IndexMap;
use serde_with::skip_serializing_none;
use serde_with::{NoneAsEmptyString, serde_as};

use serde::{Deserialize, Serialize};
use serde_json::Deserializer as JsonDeserializer;
use uuid::Uuid;

use std::fs;
use std::io::BufReader;
use std::path::PathBuf;

pub type MediaDataArrayBufferContent = MediaDataBase<Vec<u8>>;
pub type MediaDataStringContent = MediaDataBase<String>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MediaDataBase<TContentType: Serialize> {
    dictionary: String,
    path: String,
    media_type: String,
    width: u16,
    height: u16,
    content: TContentType,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum MediaType {
    ArrayBuffer(Vec<u8>),
    String(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Media<T = MediaType> {
    index: usize,
    data: T,
}

/// Represents a single term metadata entry found by find_term_meta_bulk.
/// This structure matches the output of the JavaScript _createTermMeta function.
// #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
// pub struct DatabaseTermMeta {
//     /// Index of the original query term in the input term_list_input.
//     pub index: usize,
//     /// The term expression. (Corresponds to JS row.expression, named 'term' in JS output)
//     pub term: String,
//     /// The type of metadata (e.g., Freq, Pitch, Ipa). (Corresponds to JS row.mode)
//     pub mode: TermMetaModeType,
//     /// The actual metadata content. (Corresponds to JS row.data)
//     pub data: MetaDataMatchType,
//     /// The name of the dictionary this metadata belongs to.
//     pub dictionary: String,
// }

impl From<DatabaseTermEntryTuple> for DatabaseTermEntry {
    fn from(tuple: DatabaseTermEntryTuple) -> Self {
        Self {
            id: tuple.0,
            expression: tuple.1,
            reading: tuple.2,
            expression_reverse: tuple.3,
            reading_reverse: tuple.4,
            definition_tags: tuple.5,
            tags: tuple.6,
            rules: tuple.7,
            score: tuple.8,
            glossary: tuple.9,
            sequence: tuple.10,
            term_tags: tuple.11,
            dictionary: tuple.12,
            file_path: tuple.13,
        }
    }
}

impl From<DatabaseTermEntry> for DatabaseTermEntryTuple {
    fn from(s: DatabaseTermEntry) -> Self {
        Self(
            s.id,
            s.expression,
            s.reading,
            s.expression_reverse,
            s.reading_reverse,
            s.definition_tags,
            s.tags,
            s.rules,
            s.score,
            s.glossary.to_vec(),
            s.sequence,
            s.term_tags,
            s.dictionary,
            s.file_path,
        )
    }
}

#[skip_serializing_none]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(from = "DatabaseTermEntryTuple", into = "DatabaseTermEntryTuple")]
pub struct DatabaseTermEntry {
    pub id: String,
    pub expression: String,
    pub reading: String,
    pub expression_reverse: String,
    pub reading_reverse: String,
    pub definition_tags: Option<String>,
    /// Legacy alias for the `definitionTags` field.
    pub tags: Option<String>,
    pub rules: String,
    pub score: i128,
    pub glossary: Vec<TermGlossaryGroupType>,
    pub sequence: Option<i128>,
    pub term_tags: Option<String>,
    pub dictionary: String,
    pub file_path: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DatabaseTermEntryTuple(
    // id
    pub String,
    // expression
    pub String,
    // reading
    pub String,
    // expression_reverse
    pub String,
    // reading_reverse
    pub String,
    // definition_tags
    pub Option<String>,
    // tags
    pub Option<String>,
    // rules
    pub String,
    // score
    pub i128,
    // glossary
    pub Vec<TermGlossaryGroupType>,
    // sequence
    pub Option<i128>,
    // term_tags
    pub Option<String>,
    // dictionary
    pub String,
    // file_path
    pub String,
);

/// What database field was used to match the source term.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TermSourceMatchSource {
    Term,
    Reading,
    Sequence,
}

/// How the search term relates to the final term.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TermSourceMatchType {
    Exact,
    Prefix,
    Suffix,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TermEntry {
    pub id: String,
    pub index: usize,
    pub match_type: TermSourceMatchType,
    pub match_source: TermSourceMatchSource,
    pub term: String,
    pub reading: String,
    pub definition_tags: Vec<String>,
    pub term_tags: Vec<String>,
    pub rules: Vec<String>,
    pub definitions: Vec<TermGlossaryGroupType>,
    pub score: i128,
    pub dictionary: String,
    pub sequence: i128,
}

impl DatabaseTermEntry {
    pub fn into_term_entry_specific(
        self,
        match_source: TermSourceMatchSource,
        match_type: TermSourceMatchType,
        index: usize,
    ) -> TermEntry {
        let DatabaseTermEntry {
            id,
            expression,
            reading,
            expression_reverse: _expression_reverse,
            reading_reverse: _reading_reverse,
            definition_tags,
            tags: _tags,
            rules,
            score,
            glossary,
            sequence,
            term_tags,
            dictionary,
            file_path: _file_path,
        } = self;
        TermEntry {
            id,
            index,
            match_type,
            match_source,
            term: expression,
            reading,
            definition_tags: split_optional_string_field(definition_tags),
            term_tags: split_optional_string_field(term_tags),
            rules: split_optional_string_field(Some(rules)),
            definitions: glossary,
            score,
            dictionary,
            sequence: sequence.unwrap_or(-1),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DatabaseTag {
    /// id field doesn't exist in JS
    /// need it because primary keys must be unique
    //#[serde(skip_deserializing, default)]
    pub id: String,
    pub name: String,
    pub category: String,
    pub order: u64,
    pub notes: String,
    pub score: i128,
    /// dictionary gets added afterwards
    /// it doesn't exist in any yomitan dictionary
    //#[serde(skip_deserializing, default)]
    pub dictionary: String,
}

/*************** Database Term Meta ***************/

pub trait DBMetaType {
    fn mode(&self) -> &TermMetaModeType;
    fn expression(&self) -> &str;
}

// /// A custom `Yomichan_rs`-unique, generic Database Meta model.
// ///
// /// May contain `any` or `all` of the values.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum DatabaseMetaMatchType {
    Frequency(DatabaseMetaFrequency),
    Pitch(DatabaseMetaPitch),
    Phonetic(DatabaseMetaPhonetic),
}

impl DatabaseMetaMatchType {
    pub fn convert_kanji_meta_file(
        outpath: PathBuf,
        dict_name: String,
    ) -> Result<Vec<DatabaseMetaFrequency>, DictionaryFileError> {
        let file = fs::File::open(&outpath).map_err(|reason| DictionaryFileError::FailedOpen {
            outpath: outpath.clone(),
            reason: reason.to_string(),
        })?;
        let reader = BufReader::new(file);

        // Kanji metas are only frequencies
        let mut stream =
            JsonDeserializer::from_reader(reader).into_iter::<Vec<DatabaseMetaFrequency>>();

        let mut entries = match stream.next() {
            Some(Ok(entries)) => entries,
            Some(Err(reason)) => {
                return Err(crate::errors::DictionaryFileError::File {
                    outpath,
                    reason: reason.to_string(),
                });
            }
            None => return Err(DictionaryFileError::Empty(outpath)),
        };
        entries.iter_mut().for_each(|entry| {
            entry.id = Uuid::now_v7().to_string();
            entry.dictionary = dict_name.clone();
        });
        Ok(entries)
    }

    pub fn convert_term_meta_file(
        outpath: PathBuf,
        dict_name: String,
    ) -> Result<Vec<DatabaseMetaMatchType>, DictionaryFileError> {
        let file = fs::File::open(&outpath).map_err(|reason| DictionaryFileError::FailedOpen {
            outpath: outpath.clone(),
            reason: reason.to_string(),
        })?;
        let reader = BufReader::new(file);

        let mut stream = JsonDeserializer::from_reader(reader).into_iter::<Vec<TermMeta>>();
        let entries = match stream.next() {
            Some(Ok(entries)) => entries,
            Some(Err(reason)) => {
                return Err(crate::errors::DictionaryFileError::File {
                    outpath,
                    reason: reason.to_string(),
                });
            }
            None => return Err(DictionaryFileError::Empty(outpath)),
        };

        let term_metas: Vec<DatabaseMetaMatchType> = entries
            // entries is TermMetaBank which is Vec<TermMetaData>
            .into_iter()
            .map(|entry| {
                let id = Uuid::now_v7().to_string();
                let TermMeta {
                    expression,
                    mode,
                    data,
                } = entry;

                match data {
                    MetaDataMatchType::Frequency(data) => {
                        DatabaseMetaMatchType::Frequency(DatabaseMetaFrequency {
                            id,
                            freq_expression: expression,
                            mode: TermMetaModeType::Freq,
                            data,
                            dictionary: dict_name.clone(),
                        })
                    }
                    MetaDataMatchType::Pitch(data) => {
                        DatabaseMetaMatchType::Pitch(DatabaseMetaPitch {
                            id,
                            pitch_expression: expression,
                            mode: TermMetaModeType::Pitch,
                            data,
                            dictionary: dict_name.clone(),
                        })
                    }
                    MetaDataMatchType::Phonetic(data) => {
                        DatabaseMetaMatchType::Phonetic(DatabaseMetaPhonetic {
                            id,
                            phonetic_expression: expression,
                            mode: TermMetaModeType::Ipa,
                            data,
                            dictionary: dict_name.clone(),
                        })
                    }
                }
            })
            .collect();
        Ok(term_metas)
    }
}

/// Used to store the frequency metadata of a term in the db.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DatabaseMetaFrequency {
    pub id: String,
    pub freq_expression: String,
    /// Is of type [`TermMetaModeType::Freq`]
    pub mode: TermMetaModeType,
    pub data: TermMetaFreqDataMatchType,
    pub dictionary: String,
}
impl DBMetaType for DatabaseMetaFrequency {
    fn mode(&self) -> &TermMetaModeType {
        &self.mode
    }
    fn expression(&self) -> &str {
        &self.freq_expression
    }
}

/// Used to store the pitch metadata of a term in the db.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DatabaseMetaPitch {
    pub id: String,
    pub pitch_expression: String,
    /// Is of type [`TermMetaModeType::Pitch`]
    pub mode: TermMetaModeType,
    pub data: TermMetaPitchData,
    pub dictionary: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
/// A tag represents some brief information about part of a dictionary entry.
pub struct DictionaryTag {
    /// The name of the tag.
    pub name: String,
    /// The category of the tag.
    pub category: String,
    /// A number indicating the sorting order of the tag.
    pub order: usize,
    /// A score value for the tag.
    pub score: usize,
    /// An array of descriptions for the tag. If there are multiple entries,
    /// the values will typically have originated from different dictionaries.
    /// However, there is no correlation between the length of this array and
    /// the length of the `dictionaries` field, as duplicates are removed.
    pub content: Vec<String>,
    /// An array of dictionary names that contained a tag with this name and category.
    pub dictionaries: Vec<String>,
    /// Whether or not this tag is redundant with previous tags.
    pub redundant: bool,
}
impl DictionaryTag {
    /// sets the category to "default"
    pub fn new_default(name: String, dictionary: String) -> Self {
        Self {
            name,
            category: "default".to_string(),
            order: 0,
            score: 0,
            content: vec![],
            dictionaries: vec![dictionary],
            redundant: false,
        }
    }
}

/// Pitch accent information for a term, represented as the position of the downstep.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PitchAccent {
    /// Type of the pronunciation, for disambiguation between union type members.
    /// Should be `"pitch-accent"` in the json.
    pub term: TermPronunciationMatchType,
    /// Position of the downstep, as a number of mora.
    pub position: u8,
    /// Positions of morae with a nasal sound.
    pub nasal_positions: Vec<u8>,
    /// Positions of morae with a devoiced sound.
    pub devoice_positions: Vec<u8>,
    /// Tags for the pitch accent.
    pub tags: Vec<DictionaryTag>,
}
impl DBMetaType for DatabaseMetaPitch {
    fn mode(&self) -> &TermMetaModeType {
        &self.mode
    }
    fn expression(&self) -> &str {
        &self.pitch_expression
    }
}

/// Used to store the phonetic metadata of a term in the db.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DatabaseMetaPhonetic {
    pub id: String,
    pub phonetic_expression: String,
    /// Is of type [`TermMetaModeType::Ipa`]
    pub mode: TermMetaModeType,
    pub data: TermMetaPhoneticData,
    pub dictionary: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TermMetaPhoneticData {
    pub reading: String,
    /// List of different IPA transcription information for the term and reading combination.
    pub transcriptions: Vec<PhoneticTranscription>,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PhoneticTranscription {
    /// Type of the pronunciation, for disambiguation between union type members.
    /// Should be `"phonetic-transcription"` in the json.
    pub match_type: TermPronunciationMatchType,
    /// IPA transcription for the term.
    pub ipa: String,
    /// List of tags for this IPA transcription.
    pub tags: Vec<DictionaryTag>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TermPronunciationMatchType {
    #[serde(rename = "lowercase")]
    PitchAccent,
    #[serde(rename = "phonetic-transcription")]
    PhoneticTranscription,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Pronunciation {
    PitchAccent(PitchAccent),
    PhoneticTranscription(PhoneticTranscription),
}
impl DBMetaType for DatabaseMetaPhonetic {
    fn mode(&self) -> &TermMetaModeType {
        &self.mode
    }
    fn expression(&self) -> &str {
        &self.phonetic_expression
    }
}

/*************** Database Kanji Meta ***************/

/// Kanji Meta's only have frequency data
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DatabaseKanjiMeta {
    pub character: String,
    /// Is of type [TermMetaModeType::Freq]
    pub mode: TermMetaModeType,
    pub data: TermMetaFreqDataMatchType,
    pub dictionary: String,
}

#[serde_as]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatabaseKanjiEntry {
    pub character: String,
    #[serde_as(as = "NoneAsEmptyString")]
    pub onyomi: Option<String>,
    #[serde_as(as = "NoneAsEmptyString")]
    pub kunyomi: Option<String>,
    #[serde_as(as = "NoneAsEmptyString")]
    pub tags: Option<String>,
    pub meanings: Vec<String>,
    pub stats: Option<IndexMap<String, String>>,
    /// The kanji dictionary name.
    /// Does not exist within the JSON, gets added _after_ deserialization.
    //#[serde(skip_deserializing)]
    pub dictionary: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KanjiEntry {
    pub index: usize,
    pub character: String,
    pub onyomi: Vec<String>,
    pub kunyomi: Vec<String>,
    pub tags: Vec<String>,
    pub definitions: Vec<String>,
    pub stats: IndexMap<String, String>,
    pub dictionary: String,
}

/*************** Database Dictionary ***************/

pub type DictionaryCountGroup = IndexMap<String, u16>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DictionaryCounts {
    total: Option<DictionaryCountGroup>,
    counts: Vec<DictionaryCountGroup>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DeleteDictionaryProgressData {
    count: u64,
    processed: u64,
    store_count: u16,
    stores_processed: u64, // Corrected typo: stores_processed
}

// #[derive(thiserror::Error, Debug)]
// #[error("queries returned None:\n {queries:#?}\n reason: {reason}")]
// pub struct QueryRequestError {
//     queries: Vec<QueryRequestMatchType>,
//     reason: Box<native_db::db_type::Error>,
// }

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum QueryRequestMatchType {
    TermExactQueryRequest(TermExactQueryRequest),
    GenericQueryRequest(GenericQueryRequest),
}
/// converts any `IntoIter<Enum::Variant(T)>` to a `IntoIter<Item = T>`
#[macro_export]
macro_rules! iter_variant_to_iter_type {
    ($items:expr, $enum_type:ident :: $variant:ident) => {
        $items
            .iter()
            .filter_map(|item| {
                if let $enum_type::$variant(data) = item {
                    Some(data.clone())
                } else {
                    None
                }
            })
            .collect()
    };
}
#[macro_export]
macro_rules! iter_type_to_iter_variant {
    ($items_iterable:expr, $enum_type:ident :: $variant:ident) => {
        $items_iterable
            .into_iter()
            .map(|item_to_wrap| $enum_type::$variant(item_to_wrap))
    };
}
// Collects mutable references to data within a specific enum variant from an iterable.
/// Input `$items` is expected to have an `.iter_mut()` method (e.g., a `Vec<MyEnum>`).
/// The output is a `Vec<&mut InnerDataType>`.
#[macro_export]
macro_rules! collect_variant_data_ref {
    ($items:expr, $enum_type:ident :: $variant:ident) => {
        $items
            .iter_mut() // Iterates over &mut EnumType
            .filter_map(|item_ref| {
                // item_ref_mut is &mut EnumType
                match item_ref {
                    // `ref mut data` borrows the data mutably from within the enum variant
                    $enum_type::$variant(ref data) => Some(data), // data is &mut InnerDataType
                    _ => None,                                    // Ignore other variants
                }
            })
            .collect::<Vec<_>>() // Collects into Vec<&mut InnerDataType>
    };
}
/// Converts an iterable of items into a Vec of enums, where each enum variant
/// holds a mutable reference to an original item.
/// Input `$items_iterable` is expected to have an `.iter_mut()` method (e.g., a `Vec<MyData>`).
/// The enum variant specified must be capable of holding a mutable reference
/// (e.g., defined as `enum MyWrapper<'a> { MyVariant(&'a mut MyData) }`).
/// The output is a `Vec<EnumType<'a>::Variant(&'a mut MyData)>`.
#[macro_export]
macro_rules! variant_to_generic_vec_mut {
    ($items_iterable:expr, $enum_type:ident :: $variant:ident) => {
        $items_iterable
            .iter_mut() // Iterates over &mut MyData
            .map(|item_ref_mut| {
                // item_ref_mut is &mut MyData
                // The enum variant constructor takes the mutable reference
                $enum_type::$variant(item_ref_mut)
            })
            .collect::<Vec<_>>() // Collects into Vec<EnumType::Variant(&mut MyData)>
    };
}

// #[derive(thiserror::Error, Debug)]
// pub enum DictionaryDatabaseError {
//     #[error("database error: {0}")]
//     Database(#[from] Box<native_db::db_type::Error>),
//     #[error("failed to find terms: {0}")]
//     QueryRequest(#[from] QueryRequestError),
//     #[error("incorrect variant(s) passed: {wrong:#?}\nexpected: {expected:#?}")]
//     WrongQueryRequestMatchType {
//         wrong: QueryRequestMatchType,
//         expected: QueryRequestMatchType,
//     },
// }

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TermExactQueryRequest {
    pub term: String,
    pub reading: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Eq, PartialOrd, Ord, Hash)]
pub enum QueryType {
    String(String),
    Sequence(i128),
}
/// so far it seems this can be refactored to use references
/// for now keep owned so don't have to deal with lifetimes
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Eq, PartialOrd, Ord, Hash)]
pub struct GenericQueryRequest {
    pub query_type: QueryType,
    pub dictionary: String,
}

impl GenericQueryRequest {
    pub fn new(query_type: QueryType, dictionary: &str) -> Self {
        Self {
            query_type,
            dictionary: dictionary.to_string(),
        }
    }
    pub fn from_query_type_slice_to_vec(queries: &[QueryType], dictionary: &str) -> Vec<Self> {
        queries
            .iter()
            .map(|q| Self::new(q.clone(), dictionary))
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MediaRequest {
    path: String,
    dictionary: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum FindMultiBulkDataItemType {
    String(String),
    // Consider adding other types if `item` in JS can be non-string for into_term_generic
}
impl PartialEq<FindMultiBulkDataItemType> for String {
    fn eq(&self, other: &FindMultiBulkDataItemType) -> bool {
        match other {
            FindMultiBulkDataItemType::String(s_other) => self == s_other,
            // _ => false, // If other variants are added
        }
    }
}

/// A single yomichan/yomitan dictionary's file data all parsed into rust types.
#[derive(Debug, Serialize, Deserialize)]
pub struct DatabaseDictionaryData {
    pub tag_list: Vec<DatabaseTag>,
    pub kanji_meta_list: Vec<DatabaseMetaFrequency>,
    pub kanji_list: Vec<DatabaseKanjiEntry>,
    pub term_meta_list: Vec<DatabaseMetaMatchType>,
    pub term_list: Vec<DatabaseTermEntryTuple>,
    pub summary: DictionarySummary,
}

pub fn split_optional_string_field(field: Option<String>) -> Vec<String> {
    field
        .map(|s| {
            s.split(' ')
                .map(String::from)
                .filter(|part| !part.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

pub fn split_string_field(field: String) -> Vec<String> {
    field
        .split(' ')
        .map(String::from)
        .filter(|part| !part.is_empty())
        .collect()
}
