//! # Yomichan Importer
//!
//! A library for deserializing Yomichan/Yomitan dictionary files;
//!
//! ## Usage
//! Note: rayon is ≈4.33 faster [13s vs. 3s];
//! ```toml
//! [dependencies]
//! # features = ["simd", "rayon", "trace"]
//! importer = { git = "https://github.com/aramrw/importer", features = ["rayon"] }
//! ```
//!
//! ```no_run
//! use importer::import_dictionary;
//!
//! let path = std::path::Path::new("./dictionaries/kotobankesjp");
//! let data = import_dictionary(path).unwrap();
//! std::fs::write(
//!    "./data.json",
//!    serde_json::to_string_pretty(&data).unwrap(),
//! )
//! .unwrap();
//! ```
//!
//! #### Output
//! ```
//!    1 ▼ (6) {tag_list: […], kanji_meta_list: [], kanji_list: [], term_meta_list: [], term_list: […], …}
//!     2   ▷ tag_list: (4) [{…}, {…}, {…}, {…}]
//!    40     kanji_meta_list: []
//!    41     kanji_list: []
//!    42     term_meta_list: []
//!    43   ▽ term_list: (105032) [[…], […], […], […], […], […], […], […], […], […], […], […], […], […], …]
//!    44     ▽ [0]: (14) ["01989b10-661a-7822-809c-14f28ec60200", "salmónidos", "", "sodinómlas", "", …]
//!   45         [0]: "01989b10-661a-7822-809c-14f28ec60200"
//!   46         [1]: "salmónidos"
//!   47         [2]: ""
//!   48         [3]: "sodinómlas"
//!   49         [4]: ""
//!   50         [5]: "n"
//!   51         [6]: null
//!    52         [7]: ""
//!    53         [8]: 0
//!    54       ▽ [9]: (1) [{Content: {plain_text: "- \n[男] 〘複数形〙 〖魚〗 サケ科．", html: null}}]
//!    55         ▽ [0]: (1) {Content: {plain_text: "- \n[男] 〘複数形〙 〖魚〗 サケ科．", html: null}}
//!    56           ▽ Content: (2) {plain_text: "- \n[男] 〘複数形〙 〖魚〗 サケ科．", html: null}
//!    57               plain_text: "- \n[男] 〘複数形〙 〖魚〗 サケ科．"
//!    58               html: null
//!    62         [10]: 0
//!    63         [11]: ""
//!    64         [12]: "小学館 西和中辞典 第2版"
//!    65         [13]: "./dictionaries/kotobankesjp/term_bank_10.json"
//!    67     ▷ [1]: (14) ["01989b10-661a-7822-809c-153b62011954", "salmuera", "", "areumlas", "", "n", …]
//!    90     ▷ [2]: (14) ["01989b10-661a-7822-809c-158ea4356cfb", "salmuerizada", "", "adazireumlas", …]
//!   113     ▷ [3]: (14) ["01989b10-661a-7822-809c-15a0375100ba", "salmuerizado", "", "odazireumlas", …]
//!   136     ▷ [4]: (14) ["01989b10-661a-7822-809c-15b525269db6", "salobral", "", "larbolas", "", "a…", …]
//!   159     ▷ [5]: (14) ["01989b10-661a-7822-809c-15d6a3c26560", "salobre", "", "erbolas", "", "adj", …]
//!
//! ```
//!
//! ## Modules
//!
//! - `dictionary_data`: Contains the data structures for the Yomichan dictionary format
//! - `dictionary_database`: Contains the data structures for the database format
//! - `dictionary_importer`: Contains the main logic for importing the dictionary
//! - `errors`: Contains the error types for the library
//! - `ptr`: A simple type alias for `Arc<parking_lot::RwLock>`
//! - `structured_content`: Contains the data structures for the structured content of the dictionary entries
//! - `test_utils`: Contains utility functions for testing
//! - `utils`: Contains utility functions

pub mod dictionary_data;
pub mod dictionary_database;
pub mod dictionary_importer;
pub mod errors;
// pub mod ptr;
pub mod structured_content;
// pub mod test_utils;
pub mod utils;

pub use dictionary_database::DatabaseDictionaryData;
pub use dictionary_importer::import_dictionary;

#[cfg(test)]
mod importer_tests {
    use crate::{
        dictionary_database::DatabaseDictionaryData, dictionary_importer::import_dictionary,
    };
    use tracing_subscriber;

    #[test]
    fn dict() {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();
        let path = std::path::Path::new("./dictionaries/jitendex-yomitan");
        let data: DatabaseDictionaryData = import_dictionary(path).unwrap();
        std::fs::write("./data.json", serde_json::to_string_pretty(&data).unwrap()).unwrap();
    }

    #[ignore]
    #[test]
    fn with_pprof() {
        #[cfg(target_os = "linux")]
        let guard = pprof::ProfilerGuardBuilder::default()
            .frequency(1000)
            .blocklist(&["libc", "libgcc", "pthread", "vdso"])
            .build()
            .unwrap();

        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();
        let path = std::path::Path::new("./dictionaries/jitendex-yomitan");
        let data: DatabaseDictionaryData = import_dictionary(path).unwrap();
        std::fs::write("./data.json", serde_json::to_string_pretty(&data).unwrap()).unwrap();

        #[cfg(target_os = "linux")]
        if let Ok(report) = guard.report().build() {
            let file = std::fs::File::create("flamegraph.svg").unwrap();
            report.flamegraph(file).unwrap();
        };
    }
}
