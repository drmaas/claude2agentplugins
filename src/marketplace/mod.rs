pub mod clone;
pub mod convert;

use std::path::{Path, PathBuf};

use crate::convert::{ConvertOptions, DirectoryReport};
use crate::error::Result;

pub fn convert_marketplace(
    repo_url: &str,
    branch: &str,
    output: &Path,
    options: &ConvertOptions,
) -> Result<DirectoryReport> {
    let local = PathBuf::from(repo_url);
    if local.is_dir() {
        return convert::batch_convert(&local, output, options);
    }

    let temp_dir = tempfile::tempdir()?;
    clone::clone_repo(repo_url, Some(branch), None, temp_dir.path())?;
    convert::batch_convert(temp_dir.path(), output, options)
}
