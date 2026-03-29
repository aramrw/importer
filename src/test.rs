#[cfg(test)]
mod unit_test {
    use crate::{import_dictionary, utils};

    #[test]
    fn import() {
        let zip_path = "dictionaries/kotobankesjp";
        let data = import_dictionary(zip_path).unwrap();
    }

    #[test]
    fn import_zip() {
        use crate::dictionary_importer::import_dictionary;
        use std::fs::File;
        use std::io::Write;
        use tempfile::tempdir;
        use zip::write::SimpleFileOptions;

        let source_dir = std::path::Path::new("./dictionaries/kotobankesjp");
        let temp_zip_dir = tempdir().unwrap();
        let zip_path = temp_zip_dir.path().join("test_dict.zip");

        // Create a zip file from the source directory
        let file = File::create(&zip_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);

        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .unix_permissions(0o755);

        for entry in std::fs::read_dir(source_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let name = path.file_name().unwrap().to_str().unwrap();

            if path.is_file() {
                zip.start_file(name, options).unwrap();
                let content = std::fs::read(path).unwrap();
                zip.write_all(&content).unwrap();
            }
        }
        zip.finish().unwrap();

        let data = import_dictionary(&zip_path).expect("Failed to import zip");
        assert_eq!(data.summary.title, "小学館 西和中辞典 第2版");
    }
}
