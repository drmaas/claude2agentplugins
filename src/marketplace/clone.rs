use std::path::Path;

use crate::error::Result;

pub fn clone_repo(repo_url: &str, branch: &str, dest: &Path) -> Result<()> {
    let url = format_repo_url(repo_url);

    git2::build::RepoBuilder::new()
        .branch(branch)
        .clone(&url, dest)?;

    Ok(())
}

fn format_repo_url(repo_url: &str) -> String {
    if repo_url.starts_with("http") || repo_url.starts_with("git@") {
        repo_url.to_string()
    } else {
        format!("https://github.com/{}", repo_url)
    }
}

#[cfg(test)]
mod tests {

    use super::format_repo_url;

    #[test]
    fn formats_github_url() {
        assert_eq!(format_repo_url("user/repo"), "https://github.com/user/repo");
    }

    #[test]
    fn preserves_full_url() {
        let url = "https://gitlab.com/user/repo.git";
        assert_eq!(format_repo_url(url), url);
    }

    #[test]
    fn preserves_git_url() {
        let url = "git@github.com:user/repo.git";
        assert_eq!(format_repo_url(url), url);
    }
}
