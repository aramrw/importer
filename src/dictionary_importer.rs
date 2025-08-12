//! Contains the main logic for importing a Yomichan dictionary.

use crate::dictionary_data::{
    DictionaryDataTag, TermGlossaryImage, YomichanIndexFile, dictionary_data_util,
};
use crate::dictionary_database::{
    DatabaseDictionaryData, DatabaseKanjiEntry, DatabaseMetaFrequency, DatabaseMetaMatchType,
    DatabaseTag, DatabaseTermEntry, DatabaseTermEntryTuple, MediaDataArrayBufferContent,
};
use crate::errors::{DictionaryFileError, ImportError, ImportZipError, TagBankFileError};
use crate::structured_content::TermEntryItem;

use chrono::prelude::*;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
#[cfg(not(feature = "simd"))]
use serde_json::Deserializer as JsonDeserializer;
use std::fmt::Debug;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::Instant;
use std::{fs, io};
use tempfile::tempdir;
use uuid::Uuid;

#[cfg(feature = "trace")]
use tracing::debug;

#[cfg(feature = "simd")]
use memchr::memmem;
#[cfg(feature = "rayon")]
use rayon::prelude::*;

/// The steps of the import process.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ImportSteps {
    /// The import process has not started.
    Uninitialized,
    /// Validating the index file.
    ValidateIndex,
    /// Validating the schema of the dictionary files.
    ValidateSchema,
    /// Formatting the dictionary data.
    FormatDictionary,
    /// Importing media files.
    ImportMedia,
    /// Importing data files.
    ImportData,
    /// The import process has completed.
    Completed,
}

/// The names of the compiled schema files.
#[allow(clippy::enum_variant_names)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CompiledSchemaNames {
    /// A file containing term entries.
    TermBank,
    /// Metadata & information for terms.
    /// This currently includes `frequency data` and `pitch accent` data.
    TermMetaBank,
    /// A file containing kanji entries.
    KanjiBank,
    /// A file containing kanji metadata.
    KanjiMetaBank,
    /// Data file containing tag information for terms and kanji.
    TagBank,
}

/// The 2 types of frequency dictionaries
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum FrequencyMode {
    /// Based on occurrence count.
    #[serde(rename = "occurrence-based")]
    OccurrenceBased,
    /// Based on rank.
    #[serde(rename = "rank-based")]
    RankBased,
}

/// Final details about the Dictionary and it's import process.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DictionarySummary {
    /// Name of the dictionary.
    pub title: String,
    /// Revision of the dictionary.
    /// This value is only used for displaying information.
    pub revision: String,
    /// Whether or not this dictionary contains sequencing information for related terms.
    pub sequenced: Option<bool>,
    /// The minimum Yomitan version necessary for the dictionary to function
    pub minimum_yomitan_version: Option<String>,
    /// Format of data found in the JSON data files.
    pub version: Option<u8>,
    /// Date the dictionary was added to the db.
    pub import_date: DateTime<Local>,
    /// Whether or not wildcards can be used for the search query.
    ///
    /// Rather than searching for the source text exactly,
    /// the text will only be required to be a prefix of an existing term.
    /// For example, scanning `読み` will effectively search for `読み*`
    /// which may bring up additional results such as `読み方`.
    pub prefix_wildcards_supported: bool,
    /// The counts of the dictionary.
    pub counts: SummaryCounts,
    /// The styles of the dictionary.
    pub styles: String,
    /// Whether the dictionary is updatable.
    pub is_updatable: bool,
    /// The URL of the index file.
    pub index_url: Option<String>,
    /// The URL to download the dictionary from.
    pub download_url: Option<String>,
    /// The author of the dictionary.
    pub author: Option<String>,
    /// URL for the source of the dictionary.
    pub url: Option<String>,
    /// Description of the dictionary data.
    pub description: Option<String>,
    /// Attribution information for the dictionary data.
    pub attribution: Option<String>,
    /// Language of the terms in the dictionary.
    pub source_language: Option<String>,
    /// Main language of the definitions in the dictionary.
    pub target_language: Option<String>,
    /// (See: [FrequencyMode])
    pub frequency_mode: Option<FrequencyMode>,
}

/// An error that can occur when creating a dictionary summary.
#[derive(thiserror::Error, Debug)]
pub enum DictionarySummaryError {
    /// The dictionary is incompatible with the current version of Yomitan.
    #[error(
        "dictionary is incompatible with current version of Yomitan: (${yomitan_version}; minimum required: ${minimum_required_yomitan_version}); dictionary: {dictionary}"
    )]
    IncompatibleYomitanVersion {
        yomitan_version: String,
        minimum_required_yomitan_version: String,
        dictionary: String,
    },
    /// The index data is invalid because `is_updatable` is false.
    #[error("invalid index data: `is_updatable` exists but is false")]
    InvalidIndexIsNotUpdatabale,
    /// Generic error that can mean many things went wrong
    #[error("index url: {url} is not a valid url\nreason: {err}")]
    InvalidIndexUrl { url: String, err: url::ParseError },
}

impl DictionarySummary {
    fn new(
        index: YomichanIndexFile,
        prefix_wildcards_supported: bool,
        details: SummaryDetails,
    ) -> Result<Self, DictionarySummaryError> {
        let import_date: DateTime<Local> = Local::now();
        let SummaryDetails {
            prefix_wildcard_supported: _,
            counts,
            styles,
            yomitan_version,
        } = details;
        let YomichanIndexFile {
            title,
            revision,
            sequenced,
            format: _,
            version,
            minimum_yomitan_version,
            is_updatable,
            index_url,
            download_url,
            author,
            url,
            description,
            attribution,
            source_language,
            target_language,
            frequency_mode,
            tag_meta: _,
        } = index;

        if yomitan_version == "0.0.0.0" {
            // running development version
        } else if let Some(minimum_yomitan_version) = &minimum_yomitan_version {
            if dictionary_data_util::compare_revisions(&yomitan_version, minimum_yomitan_version) {
                return Err(DictionarySummaryError::IncompatibleYomitanVersion {
                    yomitan_version,
                    minimum_required_yomitan_version: minimum_yomitan_version.clone(),
                    dictionary: title,
                });
            }
        }

        if let Some(is_updatable) = is_updatable {
            if !is_updatable {
                return Err(DictionarySummaryError::InvalidIndexIsNotUpdatabale);
            }
            if let Some(index_url) = &index_url {
                if let Err(err) = dictionary_data_util::validate_url(index_url) {
                    return Err(DictionarySummaryError::InvalidIndexUrl {
                        url: index_url.clone(),
                        err,
                    });
                }
            }
            if let Some(download_url) = &download_url {
                if let Err(err) = dictionary_data_util::validate_url(download_url) {
                    return Err(DictionarySummaryError::InvalidIndexUrl {
                        url: download_url.clone(),
                        err,
                    });
                }
            }
        }

        let res = Self {
            title,
            revision,
            sequenced,
            minimum_yomitan_version,
            version,
            import_date,
            prefix_wildcards_supported,
            counts,
            styles,
            is_updatable: is_updatable.unwrap_or_default(),
            index_url,
            download_url,
            author,
            url,
            description,
            attribution,
            source_language,
            target_language,
            frequency_mode,
        };
        Ok(res)
    }
}

/// The details of a dictionary summary.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SummaryDetails {
    /// Whether prefix wildcards are supported.
    pub prefix_wildcard_supported: bool,
    /// The counts of the dictionary.
    pub counts: SummaryCounts,
    /// The styles of the dictionary.
    pub styles: String,
    /// The version of Yomitan.
    pub yomitan_version: String,
}

/// The counts of a dictionary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SummaryCounts {
    /// The number of terms.
    pub terms: SummaryItemCount,
    /// The number of term metadata entries.
    pub term_meta: SummaryMetaCount,
    /// The number of kanji.
    pub kanji: SummaryItemCount,
    /// The number of kanji metadata entries.
    pub kanji_meta: SummaryMetaCount,
    /// The number of tag metadata entries.
    pub tag_meta: SummaryItemCount,
    /// The number of media files.
    pub media: SummaryItemCount,
}

impl SummaryCounts {
    fn new(
        term_len: usize,
        term_meta_len: usize,
        tag_len: usize,
        kanji_len: usize,
        kanji_meta_len: usize,
        term_meta_counts: MetaCounts,
        kanji_meta_counts: MetaCounts,
    ) -> Self {
        Self {
            terms: SummaryItemCount {
                total: term_len as u16,
            },
            term_meta: SummaryMetaCount {
                total: term_meta_len as u16,
                meta: term_meta_counts,
            },
            tag_meta: SummaryItemCount {
                total: tag_len as u16,
            },
            kanji_meta: SummaryMetaCount {
                total: kanji_meta_len as u16,
                meta: kanji_meta_counts,
            },
            kanji: SummaryItemCount {
                total: kanji_len as u16,
            },
            // Can't deserialize media (yet).
            media: SummaryItemCount { total: 0 },
        }
    }
}

/// The count of a summary item.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SummaryItemCount {
    /// The total number of items.
    pub total: u16,
}

impl SummaryItemCount {}

/// The count of a summary metadata item.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SummaryMetaCount {
    /// The total number of items.
    pub total: u16,
    /// The counts of the metadata types.
    pub meta: MetaCounts,
}

/// The counts of the metadata types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub struct MetaCounts {
    /// The number of frequency metadata entries.
    pub freq: u32,
    /// The number of pitch metadata entries.
    pub pitch: u32,
    /// The number of IPA metadata entries.
    pub ipa: u32,
}

impl MetaCounts {
    fn count_kanji_metas(kanji_metas: &[DatabaseMetaFrequency]) -> Self {
        MetaCounts {
            freq: kanji_metas.len() as u32,
            ..Default::default()
        }
    }
    fn count_term_metas(metas: &[DatabaseMetaMatchType]) -> Self {
        let mut meta_counts = MetaCounts::default();

        for database_meta_match_type in metas.iter() {
            match database_meta_match_type {
                DatabaseMetaMatchType::Frequency(_) => meta_counts.freq += 1,
                DatabaseMetaMatchType::Pitch(_) => meta_counts.pitch += 1,
                DatabaseMetaMatchType::Phonetic(_) => meta_counts.ipa += 1,
            }
        }

        meta_counts
    }
}

/// The type of image import.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ImageImportMatchType {
    /// An image.
    Image,
    /// An image in structured content.
    StructuredContentImage,
}

/// A requirement for importing an image.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImageImportRequirement {
    /// This is of type [`ImageImportType::Image`]
    image_type: ImageImportMatchType,
    target: TermGlossaryImage,
    source: TermGlossaryImage,
    entry: DatabaseTermEntry,
}

/// A requirement for importing an image from structured content.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StructuredContentImageImportRequirement {
    /// This is of type [`ImageImportType::StructuredContentImage`]
    image_type: ImageImportMatchType,
    target: TermGlossaryImage,
    source: TermGlossaryImage,
    entry: DatabaseTermEntry,
}

/// The context for an import requirement.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImportRequirementContext {
    //file_map: ArchiveFileMap,
    media: IndexMap<String, MediaDataArrayBufferContent>,
}
/// Deserializable type mapping a `kanji_bank_$i.json` file.
pub type KanjiBank = Vec<DatabaseKanjiEntry>;

fn extract_dict_zip<P: AsRef<std::path::Path>>(
    zip_path: P,
) -> Result<std::path::PathBuf, ImportZipError> {
    let temp_dir = tempdir()?;
    let temp_dir_path = temp_dir.path().to_owned();
    let temp_dir_path_clone = temp_dir_path.clone();

    {
        let file = fs::File::open(zip_path)?;
        let mut archive = zip::ZipArchive::new(file)?;
        let extract_handle = std::thread::spawn(move || archive.extract(temp_dir_path_clone));
        extract_handle.join().unwrap().unwrap();
    }

    temp_dir.close()?;
    Ok(temp_dir_path)
}

/// Imports a Yomichan dictionary from a zip or folder
///
/// # Arguments
///
/// * `zip_path` - The path to the zip file or unzipped folder
///
/// # Returns
///
/// A `Result` containing the imported dictionary data or an error.
pub fn import_dictionary<P: AsRef<Path> + Debug>(
    zip_path: P,
) -> Result<DatabaseDictionaryData, ImportError> {
    #[cfg(feature = "trace")]
    debug!("{zip_path:?}");
    let data: DatabaseDictionaryData = prepare_dictionary(zip_path)?;
    Ok(data)
}

/// Processes paths in parallel using Rayon.
#[cfg(feature = "rayon")]
pub fn process_paths<P, F, T, E>(paths: P, map_fn: F) -> Result<Vec<T>, E>
where
    P: IntoParallelIterator,
    F: Fn(P::Item) -> Result<Vec<T>, E> + Send + Sync,
    P::Item: Send,
    T: Send,
    E: Send,
{
    // Collect the results from each parallel task.
    let nested_result: Result<Vec<Vec<T>>, E> = paths.into_par_iter().map(map_fn).collect();

    // If successful, flatten the Vec<Vec<T>> into a single Vec<T>.
    nested_result.map(|vec_of_vecs| vec_of_vecs.into_iter().flatten().collect())
}

/// Processes paths sequentially (fallback when rayon feature is disabled).
#[cfg(not(feature = "rayon"))]
pub fn process_paths<P, F, T, E>(paths: P, map_fn: F) -> Result<Vec<T>, E>
where
    P: IntoIterator,
    F: Fn(P::Item) -> Result<Vec<T>, E>,
{
    // Collect the results from each sequential task.
    let nested_result: Result<Vec<Vec<T>>, E> = paths.into_iter().map(map_fn).collect();

    // If successful, flatten the Vec<Vec<T>> into a single Vec<T>.
    nested_result.map(|vec_of_vecs| vec_of_vecs.into_iter().flatten().collect())
}

pub fn prepare_dictionary<P: AsRef<Path>>(
    zip_path: P,
) -> Result<DatabaseDictionaryData, ImportError> {
    let mut index_path = PathBuf::with_capacity(50);
    let mut tag_bank_paths: Vec<PathBuf> = Vec::with_capacity(1);
    let mut kanji_meta_bank_paths: Vec<PathBuf> = Vec::with_capacity(1);
    let mut kanji_bank_paths: Vec<PathBuf> = Vec::with_capacity(1);
    let mut term_meta_bank_paths: Vec<PathBuf> = Vec::with_capacity(5);
    let mut term_bank_paths: Vec<PathBuf> = Vec::with_capacity(5);

    read_dir_helper(
        zip_path,
        &mut index_path,
        &mut tag_bank_paths,
        &mut kanji_meta_bank_paths,
        &mut kanji_bank_paths,
        &mut term_meta_bank_paths,
        &mut term_bank_paths,
    )?;

    let index = YomichanIndexFile::convert_index_file(index_path)?;
    let dict_name = index.title.clone();

    // Use the macro for all repeating blocks
    let tag_list: Vec<DatabaseTag> = convert_tag_bank_files(tag_bank_paths, &dict_name)?
        .into_iter()
        .flatten()
        .collect();

    let term_list: Vec<DatabaseTermEntryTuple> = process_paths(term_bank_paths, |path| {
        convert_term_bank_file(path, &dict_name)
    })?;

    let kanji_meta_list: Vec<DatabaseMetaFrequency> =
        process_paths(kanji_meta_bank_paths, |path| {
            DatabaseMetaMatchType::convert_kanji_meta_file(path, dict_name.clone())
        })?;

    let term_meta_list: Vec<DatabaseMetaMatchType> = process_paths(term_meta_bank_paths, |path| {
        DatabaseMetaMatchType::convert_term_meta_file(path, dict_name.clone())
    })?;

    let kanji_list: Vec<DatabaseKanjiEntry> = process_paths(kanji_bank_paths, |path| {
        convert_kanji_bank(path, &dict_name)
    })?;

    // The rest of the function remains the same...
    let term_meta_counts = MetaCounts::count_term_metas(&term_meta_list);
    let kanji_meta_counts = MetaCounts::count_kanji_metas(&kanji_meta_list);

    let counts = SummaryCounts::new(
        term_list.len(),
        term_meta_list.len(),
        tag_list.len(),
        kanji_list.len(),
        kanji_meta_list.len(),
        term_meta_counts,
        kanji_meta_counts,
    );

    let yomitan_version = env!("CARGO_PKG_VERSION").to_string();
    let summary_details = SummaryDetails {
        prefix_wildcard_supported: false,
        counts,
        // TODO: need to parse 'styles.css' file if it exists
        styles: "".to_string(),
        yomitan_version,
    };
    let summary = DictionarySummary::new(index, false, summary_details)?;

    Ok(DatabaseDictionaryData {
        tag_list,
        kanji_meta_list,
        kanji_list,
        term_meta_list,
        term_list,
        summary,
    })
}

// this one should probabaly be refactored to:
// 1. include the file and err if it throws like the rest of the converts
// 2. only handle one file and have the iteration be handled in the caller function
fn convert_tag_bank_files(
    outpaths: Vec<PathBuf>,
    dictionary: &str,
) -> Result<Vec<Vec<DatabaseTag>>, TagBankFileError> {
    outpaths
        .into_iter()
        .map(|p| {
            let tag_str = fs::read_to_string(p)?;
            let tag: Vec<DictionaryDataTag> = serde_json::from_str(&tag_str)?;
            let res = tag
                .into_iter()
                .map(|tag| {
                    let DictionaryDataTag {
                        name,
                        category,
                        order,
                        notes,
                        score,
                    } = tag;
                    DatabaseTag {
                        id: Uuid::now_v7().to_string(),
                        name,
                        category,
                        order,
                        notes,
                        score,
                        dictionary: dictionary.to_string(),
                    }
                })
                .collect();
            Ok(res)
        })
        .collect()
}

/****************** Kanji Bank Functions ******************/

fn convert_kanji_bank(
    outpath: PathBuf,
    dict_name: &str,
) -> Result<Vec<DatabaseKanjiEntry>, DictionaryFileError> {
    #[cfg(not(feature = "simd"))]
    let mut entries = {
        let file = fs::File::open(&outpath).map_err(|reason| DictionaryFileError::FailedOpen {
            outpath: outpath.clone(),
            reason: reason.to_string(),
        })?;
        let reader = BufReader::new(file);

        let mut stream = JsonDeserializer::from_reader(reader).into_iter::<KanjiBank>();
        match stream.next() {
            Some(Ok(entries)) => entries,
            Some(Err(reason)) => {
                return Err(crate::errors::DictionaryFileError::File {
                    outpath,
                    reason: reason.to_string(),
                });
            }
            None => return Err(DictionaryFileError::Empty(outpath)),
        }
    };

    #[cfg(feature = "simd")]
    let mut entries: KanjiBank = {
        let mut json_string =
            fs::read_to_string(&outpath).map_err(|reason| DictionaryFileError::FailedOpen {
                outpath: outpath.clone(),
                reason: reason.to_string(),
            })?;
        let json_bytes = unsafe { json_string.as_bytes_mut() };
        simd_json::from_slice(json_bytes).map_err(|err| {
            crate::errors::DictionaryFileError::File {
                outpath: outpath.clone(),
                reason: err.to_string(),
            }
        })?
    };

    for item in &mut entries {
        item.dictionary = Some(dict_name.to_owned())
    }

    Ok(entries)
}

/****************** Term Bank Functions ******************/

fn convert_term_bank_file(
    outpath: PathBuf,
    dict_name: &str,
) -> Result<Vec<DatabaseTermEntryTuple>, DictionaryFileError> {
    #[cfg(feature = "trace")]
    debug!(
        "deserializing: {:?}",
        outpath
            .file_name()
            .unwrap_or(Path::new("<unknown>").as_os_str())
    );

    #[cfg(not(feature = "simd"))]
    let entries: Vec<TermEntryItem> = {
        let file = fs::File::open(&outpath).map_err(|reason| DictionaryFileError::FailedOpen {
            outpath: outpath.clone(),
            reason: reason.to_string(),
        })?;
        let reader = BufReader::new(file);

        let mut stream = JsonDeserializer::from_reader(reader).into_iter::<Vec<TermEntryItem>>();
        match stream.next() {
            Some(Ok(entries)) => entries,
            Some(Err(reason)) => {
                eprintln!("{outpath:?} failed:\nreason: {reason}");
                return Err(crate::errors::DictionaryFileError::File {
                    outpath,
                    reason: reason.to_string(),
                });
            }
            None => return Err(DictionaryFileError::Empty(outpath)),
        }
    };

    #[cfg(feature = "simd")]
    let entries: Vec<TermEntryItem> = {
        let mut json_string =
            fs::read_to_string(&outpath).map_err(|reason| DictionaryFileError::FailedOpen {
                outpath: outpath.clone(),
                reason: reason.to_string(),
            })?;
        let json_bytes = unsafe { json_string.as_bytes_mut() };
        simd_json::from_slice(json_bytes).map_err(|err| {
            crate::errors::DictionaryFileError::File {
                outpath: outpath.clone(),
                reason: err.to_string(),
            }
        })?
    };

    // Beginning of each word/phrase/expression (entry)
    // ie: ["headword","reading","","",u128,[{/* main */}]]];
    let terms: Vec<DatabaseTermEntryTuple> = entries
        .into_iter()
        .map(|entry| {
            let TermEntryItem {
                expression,
                reading,
                def_tags,
                rules,
                score,
                structured_content,
                sequence,
                term_tags,
            } = entry;
            let id = uuid::Uuid::now_v7().to_string();
            let expression_reverse = rev_str(&expression);
            let reading_reverse = rev_str(&reading);
            DatabaseTermEntryTuple(
                id,
                expression.to_string(),
                reading.to_string(),
                expression_reverse,
                reading_reverse,
                def_tags.map(|s| s.to_string()),
                None, // tags field is not in TermEntryItem
                rules.to_string(),
                score,
                structured_content.into_iter().map(|sc| sc.into()).collect(),
                Some(sequence),
                Some(term_tags.to_string()),
                dict_name.to_owned(),
                outpath.clone().to_string_lossy().to_string(),
            )
        })
        .collect();
    Ok(terms)
}

fn rev_str(expression: &str) -> String {
    expression.chars().rev().collect()
}

/****************** Helper Functions ******************/

// This function is crazy fast
fn read_dir_helper<P: AsRef<Path>>(
    zip_path: P,
    index: &mut PathBuf,
    tag_banks: &mut Vec<PathBuf>,
    kanji_meta_banks: &mut Vec<PathBuf>,
    kanji_banks: &mut Vec<PathBuf>,
    term_meta_banks: &mut Vec<PathBuf>,
    term_banks: &mut Vec<PathBuf>,
) -> Result<(), io::Error> {
    //let instant = Instant::now();

    #[cfg(not(feature = "simd"))]
    fn contains(path: &[u8], substr: &[u8]) -> bool {
        if path.starts_with(substr) {
            return true;
        }
        path.windows(substr.len()).any(|w| w == substr)
    }

    #[cfg(feature = "simd")]
    fn contains(path: &[u8], substr: &[u8]) -> bool {
        memmem::find(path, substr).is_some()
    }

    fs::read_dir(&zip_path)?.try_for_each(|entry| -> Result<(), io::Error> {
        let entry = entry?;
        let outpath_buf = entry.path();
        let outpath = outpath_buf.as_os_str().as_encoded_bytes();

        if outpath.iter().last() != Some(&b'/') {
            if contains(outpath, b"term_bank") {
                term_banks.push(outpath_buf);
            } else if contains(outpath, b"index.json") {
                *index = outpath_buf;
            } else if contains(outpath, b"term_meta_bank") {
                term_meta_banks.push(outpath_buf);
            } else if contains(outpath, b"kanji_meta_bank") {
                kanji_meta_banks.push(outpath_buf);
            } else if contains(outpath, b"kanji_bank") {
                kanji_banks.push(outpath_buf);
            } else if contains(outpath, b"tag_bank") {
                tag_banks.push(outpath_buf);
            }
        }
        Ok(())
    })?;

    //debug!("read_dir_helper: {:.3}ms", instant.elapsed().as_millis());
    Ok(())
}
