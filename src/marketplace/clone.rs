use std::path::Path;

use crate::error::Result;

pub fn clone_repo(
    repo_url: &str,
    reference: Option<&str>,
    sha: Option<&str>,
    dest: &Path,
) -> Result<()> {
    let url = format_repo_url(repo_url);
    git2::build::RepoBuilder::new().clone(&url, dest)?;

    let target = sha
        .filter(|s| !s.is_empty())
        .or_else(|| reference.filter(|r| !r.is_empty()));

    if let Some(target) = target {
        checkout_pinned(dest, target)?;
    }
    Ok(())
}

/// Check out a branch, tag, or commit SHA. The sha takes precedence over the
/// ref (matching Claude marketplace pin semantics).
fn checkout_pinned(dest: &Path, target: &str) -> Result<()> {
    let repo = git2::Repository::open(dest)?;

    let object = match repo.revparse_single(target) {
        Ok(object) => object,
        Err(first_err) => {
            let mut fetched = false;
            for spec in [
                format!("+refs/heads/{target}:refs/remotes/origin/{target}"),
                format!("+refs/tags/{target}:refs/tags/{target}"),
            ] {
                let mut fetch_options = git2::FetchOptions::new();
                if let Ok(mut remote) = repo.find_remote("origin")
                    && remote
                        .fetch(&[&spec], Some(&mut fetch_options), None)
                        .is_ok()
                {
                    fetched = true;
                }
            }
            if fetched && let Ok(object) = repo.revparse_single(target) {
                object
            } else {
                return Err(first_err.into());
            }
        }
    };

    repo.set_head_detached(object.id())?;
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))?;
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
