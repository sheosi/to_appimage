use std::{path::{Path, PathBuf}, str::FromStr};

use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("The license couldn't be reconized")]
    Unrecognizable,

    #[error("License file couldn't be found")]
    NoLicenseFile,

    #[error("Couldn't read")]
    CouldntRead(#[from]std::io::Error),
}

#[derive(Serialize)]
pub enum License {
    #[serde(rename = "CC0-1.0")]
    CC0, 

    #[serde(rename = "UPL-1.0")]
    UniversalPermisiveLicense, 
    
    #[serde(rename = "MIT")]
    Mit
}

impl License {
    pub fn locate(path: &Path) -> Result<Self, Error> {
        fn is_license(p: &PathBuf) -> bool {
            p.is_file() && p.file_name().unwrap_or_default().to_ascii_lowercase() == "license"
        }

        let file = std::fs::read_dir(path)
        .unwrap()
        .flatten()
        .map(|d| d.path())
        .filter(is_license)
        .next();

    if let Some(file) = file {
        std::fs::read_to_string(file)?.parse().map_err(|_|Error::Unrecognizable)

    } else {
        Err(Error::NoLicenseFile)
    }

    }
}

impl FromStr for License {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.contains("The Universal Permissive License (UPL), Version 1.0") {Ok(License::UniversalPermisiveLicense)}
        else if s.contains("The MIT License (Expat)") {Ok(License::Mit)}
        else {Err(())}
    }
}