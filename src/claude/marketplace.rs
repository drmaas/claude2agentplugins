use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MarketplaceEntry {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub source: MarketplaceSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum MarketplaceSource {
    #[serde(rename = "github")]
    GitHub {
        repo: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        subdir: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
    },
    #[serde(rename = "url")]
    Url { url: String },
    #[serde(rename = "git-subdir")]
    GitSubdir {
        repo: String,
        subdir: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
    },
    #[serde(rename = "npm")]
    Npm { package: String },
    #[serde(rename = "pip")]
    Pip { package: String },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Marketplace {
    pub entries: Vec<MarketplaceEntry>,
}

impl Marketplace {
    pub fn parse(path: &std::path::Path) -> crate::error::Result<Self> {
        let marketplace_path = path.join(".claude-plugin").join("marketplace.json");
        if !marketplace_path.exists() {
            return Err(crate::error::Error::NotFound {
                path: marketplace_path.display().to_string(),
                reason: "No marketplace.json found".to_string(),
            });
        }
        let content = std::fs::read_to_string(&marketplace_path)?;
        let entries: Vec<MarketplaceEntry> = serde_json::from_str(&content).map_err(|e| {
            crate::error::Error::Conversion(format!("Failed to parse marketplace.json: {e}"))
        })?;
        Ok(Marketplace { entries })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_github_source() {
        let json = r#"[{"name": "test", "source": {"type": "github", "repo": "user/repo"}}]"#;
        let entries: Vec<MarketplaceEntry> = serde_json::from_str(json).unwrap();
        assert_eq!(entries.len(), 1);
        match &entries[0].source {
            MarketplaceSource::GitHub { repo, .. } => assert_eq!(repo, "user/repo"),
            _ => panic!("expected GitHub source"),
        }
    }

    #[test]
    fn parse_git_subdir_source() {
        let json = r#"[{"name": "test", "source": {"type": "git-subdir", "repo": "user/repo", "subdir": "plugins/mine"}}]"#;
        let entries: Vec<MarketplaceEntry> = serde_json::from_str(json).unwrap();
        match &entries[0].source {
            MarketplaceSource::GitSubdir { repo, subdir, .. } => {
                assert_eq!(repo, "user/repo");
                assert_eq!(subdir, "plugins/mine");
            }
            _ => panic!("expected GitSubdir source"),
        }
    }
}
