# ZIP File Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add "drop-in" support for ZIP files to the `import_dictionary` function.

**Architecture:** Use `tempfile::TempDir` to manage the lifetime of extracted files. `import_dictionary` will detect if the input is a file and, if so, extract it to a temporary directory before processing.

**Tech Stack:** Rust, `zip` crate, `tempfile` crate.

---

### Task 1: Refactor `extract_dict_zip`

**Files:**
- Modify: `src/dictionary_importer.rs`

- [ ] **Step 1: Update `extract_dict_zip` signature and implementation**

```rust
fn extract_dict_zip<P: AsRef<std::path::Path>>(
    zip_path: P,
) -> Result<(tempfile::TempDir, std::path::PathBuf), ImportZipError> {
    let temp_dir = tempdir()?;
    let temp_dir_path = temp_dir.path().to_owned();
    let temp_dir_path_clone = temp_dir_path.clone();

    {
        let file = fs::File::open(zip_path)?;
        let mut archive = zip::ZipArchive::new(file)?;
        archive.extract(temp_dir_path_clone)?;
    }

    Ok((temp_dir, temp_dir_path))
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check`
Expected: PASS (with warnings about unused return value in `import_dictionary`)

- [ ] **Step 3: Commit**

```bash
git add src/dictionary_importer.rs
git commit -m "refactor: update extract_dict_zip to return TempDir"
```

---

### Task 2: Update `import_dictionary` to handle ZIP files

**Files:**
- Modify: `src/dictionary_importer.rs`

- [ ] **Step 1: Update `import_dictionary` logic**

```rust
pub fn import_dictionary<P: AsRef<Path>>(
    path: P,
) -> Result<DatabaseDictionaryData, ImportError> {
    let path = path.as_ref();
    #[cfg(feature = "trace")]
    debug!("{path:?}");

    if path.is_dir() {
        let data: DatabaseDictionaryData = prepare_dictionary(path)?;
        Ok(data)
    } else {
        let (_temp_dir, extracted_path) = extract_dict_zip(path)?;
        let data: DatabaseDictionaryData = prepare_dictionary(extracted_path)?;
        Ok(data)
    }
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/dictionary_importer.rs
git commit -m "feat: support ZIP files in import_dictionary"
```

---

### Task 3: Add test case for ZIP import

**Files:**
- Modify: `src/test.rs`

- [ ] **Step 1: Add a test case that uses a ZIP file**
I will use the existing dictionary in `dictionaries/kotobankesjp` to create a temporary ZIP for the test.

```rust
#[test]
fn import_zip() {
    use std::fs::File;
    use std::io::Write;
    use zip::write::FileOptions;
    use tempfile::tempdir;

    let source_dir = std::path::Path::new("./dictionaries/kotobankesjp");
    let temp_zip_dir = tempdir().unwrap();
    let zip_path = temp_zip_dir.path().join("test_dict.zip");

    // Create a zip file from the source directory
    let file = File::create(&zip_path).unwrap();
    let mut zip = zip::ZipArchive::new(file).is_err().then(|| {
        let file = File::create(&zip_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        
        let options = FileOptions::default()
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
    });

    let data = import_dictionary(&zip_path).unwrap();
    assert_eq!(data.summary.title, "小学館 西和中辞典 第2版");
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test import_zip`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/test.rs
git commit -m "test: add ZIP import test case"
```
