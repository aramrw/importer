# Design Doc: ZIP File Support for Yomichan Importer

Adding "drop-in" support for ZIP files so that `import_dictionary` can handle both directories and ZIP archives seamlessly.

## Problem Statement
Currently, `import_dictionary` assumes the provided path is a directory. If a user provides a ZIP file, `read_dir_helper` fails because it tries to iterate over a file as if it were a directory. While `extract_dict_zip` exists, it is flawed (it deletes the temporary directory before it can be used) and is not integrated into the main entry point.

## Proposed Changes

### 1. Update `import_dictionary`
Modify `import_dictionary` to detect if the input path is a file or a directory:
- If it's a directory: Call `prepare_dictionary` directly.
- If it's a file: Attempt to extract it as a ZIP to a temporary directory, then call `prepare_dictionary` on that temporary directory.

### 2. Refactor `extract_dict_zip`
The current `extract_dict_zip` function returns a `PathBuf` but drops the `TempDir` object, causing immediate deletion of the files.
- Refactor it to return `Result<(TempDir, PathBuf), ImportZipError>` or handle extraction within `import_dictionary` where the `TempDir` lifetime can be managed.
- Ensure it properly extracts all files from the Yomichan ZIP structure.

### 3. Cleanup Strategy
Use `tempfile::TempDir` to ensure that extracted files are automatically deleted when the `TempDir` object goes out of scope at the end of `import_dictionary`. Since `DatabaseDictionaryData` contains only owned data, no references to the temporary files will remain.

### 4. Error Handling
Update `ImportZipError` if necessary to provide better feedback when a ZIP file is invalid or extraction fails.

## Data Flow
1. User calls `import_dictionary(path)`.
2. `import_dictionary` checks `path.is_dir()`.
3. If not a directory, `extract_dict_zip(path)` creates a `TempDir` and extracts the archive.
4. `prepare_dictionary(extracted_path)` processes the files.
5. `DatabaseDictionaryData` is returned.
6. `TempDir` goes out of scope and deletes the extracted files.

## Testing Strategy
- **Unit Test:** Create a mock ZIP file in a test and verify that `import_dictionary` can process it correctly.
- **Regression Test:** Ensure that importing from a directory still works as expected.
- **Error Case:** Verify that providing a non-existent path or an invalid ZIP file returns the appropriate `ImportZipError`.
