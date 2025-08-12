//! Contains the error types for the library.

use crate::{dictionary_importer::DictionarySummaryError};
use std::{
    io,
    path::{Path, PathBuf},
};
use thiserror::Error;

/// Abstraction over results for
pub enum YomichanResult<T> {
    /// A successful result.
    Result(T),
    /// An error.
    Err(YomichanError),
}

/// All possible `yomichan_rs` [Error] paths
#[derive(Error, Debug)]
pub enum YomichanError {
    /// An import error.
    #[error("(-)[yc_error::import] -> \n{0}")]
    Import(#[from] ImportError),
    /// A database error.
    #[error("(-)[yc_error::db]")]
    Database(#[from] DBError),
    // #[error("(-)[yc_error::profile]")]
    // Profile(#[from] ProfileError),
}

/// An error that can occur when importing a zip file.
#[derive(Error, Debug)]
pub enum ImportZipError {
    /// The zip path does not exist.
    #[error("the zip path: `{0}` does not exist")]
    DoesNotExist(PathBuf),
    /// A zip crate error.
    #[error("zip crate error: {0}")]
    ZipCrate(#[from] zip::result::ZipError),
    /// A filesystem IO error.
    #[error("filesystemIO error: {0}")]
    Io(#[from] std::io::Error),
}

impl ImportZipError {
    pub fn check_zip_paths(paths: &[impl AsRef<Path>]) -> Result<(), Self> {
        for zp in paths {
            let zp = zp.as_ref();
            if !zp.exists() {
                return Err(Self::DoesNotExist(zp.to_path_buf()));
            }
        }
        Ok(())
    }
}

/// An error that can occur when reading a dictionary file.
#[derive(Error, Debug)]
pub enum DictionaryFileError {
    /// Failed to deserialize a file.
    #[error("failed to deserialize file: `{outpath}`\nreason: {reason}")]
    File { outpath: PathBuf, reason: String },
    /// The file is empty.
    #[error("no data in term_bank stream, is the file empty? file: {0}")]
    Empty(PathBuf),
    /// Failed to open a file.
    #[error("failed to open file: {outpath}\nreason: {reason}")]
    FailedOpen { outpath: PathBuf, reason: String },
}

/// An error that can occur when reading a tag bank file.
#[derive(Error, Debug)]
pub enum TagBankFileError {
    /// An IO error.
    #[error("{0}")]
    Io(#[from] io::Error),
    /// A JSON error.
    #[error("{0}")]
    Json(#[from] serde_json::Error),
}

/// An error that can occur when importing a dictionary.
#[derive(Error, Debug)]
pub enum ImportError {
    /// The dictionary already exists.
    #[error(
        "cannot import {0} as it is already installed\n[help]: if you are attempting to update it, first call `Yomichan::delete_dictionaries(&self, names: &[&{0}])`, and try importing again"
    )]
    DictionaryAlreadyExists(String),
    /// A dictionary file error.
    #[error("dictionary file error: {0}")]
    DictionaryFile(#[from] DictionaryFileError),
    /// A zip error.
    #[error("{0}")]
    Zip(#[from] ImportZipError),
    /// An IO error.
    #[error("db err: {0}")]
    IO(#[from] std::io::Error),
    /// A JSON error.
    #[error("json err: {0}")]
    Json(#[from] serde_json::error::Error),
    /// A thread error.
    #[error("thread err: {0}")]
    ThreadErr(#[from] std::thread::AccessError),
    /// An error with a line number.
    #[error("error at line {0}: {1}")]
    LineErr(u32, Box<ImportError>),
    /// A custom error.
    #[error("json err: {0}")]
    Custom(String),
    /// Invalid JSON.
    #[error("failed to deserialize file: {file}, reason: {e:#?}")]
    InvalidJson { file: PathBuf, e: Option<String> },
    /// Failed to create a summary.
    #[error("failed to create summary: {0}")]
    Summary(#[from] DictionarySummaryError),
    // #[error("profile error: {0}")]
    // Profile(#[from] ProfileError),
    /// A tag bank file error.
    #[error("[tag-bank-file error]: {0}")]
    TagBankFile(#[from] TagBankFileError),
}

/// A database error.
#[derive(Error, Debug)]
pub enum DBError {
    /// A query error.
    #[error("query err: {0}")]
    Query(String),
    /// No results found.
    #[error("none found err: {0}")]
    NoneFound(String),
    /// An import error.
    #[error("import err: {0}")]
    Import(#[from] ImportError),
    // #[error("(-)[yc_error::profile]")]
    // Profile(#[from] ProfileError),
}

#[macro_export]
macro_rules! try_with_line {
    () => {
        macro_rules! line_number {
            () => {
                line!()
            };
        }

        ($expr:expr) => {
            match $expr {
                Ok(val) => val,
                Err(err) => return Err(errors::ImportError::from((line_number!(), err))),
            }
        };
    };
}

impl From<(u32, std::io::Error)> for ImportError {
    fn from(err: (u32, std::io::Error)) -> ImportError {
        ImportError::LineErr(err.0, Box::new(ImportError::from(err.1)))
    }
}

impl From<(u32, serde_json::error::Error)> for ImportError {
    fn from(err: (u32, serde_json::error::Error)) -> ImportError {
        ImportError::LineErr(err.0, Box::new(ImportError::from(err.1)))
    }
}

pub mod error_helpers {
    /// # Example
    ///
    /// ```
    /// #[error("[error::{}]", fmterr_module(vec!["main", "database"]))]
    /// // [error::main::database]
    /// ```
    pub fn fmterr_module(mods: Vec<&str>) -> String {
        mods.join("::")
    }

    /// A helper macro to create a standard module error message attribute.
    #[macro_export]
    macro_rules! fmt_mod_error {
    ( $($path_part:literal),* ) => {
        // This macro expands to the full #[error(...)] attribute
        #[error("[{}]", error_helpers::fmterr_module(&[ $($path_part),* ]))]
    };
}
}
