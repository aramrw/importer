///! # Usage
/// ```no_run
/// let path = std::path::Path::new("./dictionaries/kotobankesjp");
///    let data: DatabaseDictionaryData = import_dictionary(path).unwrap();
///    std::fs::write(
///      "./data.json",
///    serde_json::to_string_pretty(&data).unwrap(),
///  )
/// .unwrap();
/// ```

mod dictionary_data;
mod dictionary_database;
mod dictionary_importer;
// mod settings;
mod errors;
mod ptr;
mod structured_content;
mod test_utils;
mod utils;

#[cfg(test)]
mod importer_tests {
    use crate::{dictionary_database::DatabaseDictionaryData, dictionary_importer::import_dictionary};

    #[test]
    fn dict() {
        #[cfg(target_os = "linux")]
        let guard = pprof::ProfilerGuardBuilder::default()
            .frequency(1000)
            .blocklist(&["libc", "libgcc", "pthread", "vdso"])
            .build()
            .unwrap();

        let path = std::path::Path::new("./dictionaries/kotobankesjp");
        let data: DatabaseDictionaryData = import_dictionary(path).unwrap();

        std::fs::write(
            "./data.json",
            serde_json::to_string_pretty(&data).unwrap(),
        )
        .unwrap();

        #[cfg(target_os = "linux")]
        if let Ok(report) = guard.report().build() {
            let file = std::fs::File::create("flamegraph.svg").unwrap();
            report.flamegraph(file).unwrap();
        };
    }
}
