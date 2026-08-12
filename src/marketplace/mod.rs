pub mod clone;
pub mod convert;

use std::path::Path;

use crate::error::Result;

pub fn convert_marketplace(
    repo_url: &str,
    branch: &str,
    output: &Path,
    extension_namespace: &str,
    strict: bool,
) -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    clone::clone_repo(repo_url, branch, temp_dir.path())?;
    convert::batch_convert(temp_dir.path(), output, extension_namespace, strict)?;
    Ok(())
}
