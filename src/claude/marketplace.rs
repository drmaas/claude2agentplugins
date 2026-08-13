use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Marketplace {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MarketplaceMetadata>,
    pub plugins: Vec<MarketplaceEntry>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct MarketplaceMetadata {
    #[serde(rename = "pluginRoot", skip_serializing_if = "Option::is_none")]
    pub plugin_root: Option<String>,
}

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
#[serde(untagged)]
pub enum MarketplaceSource {
    /// Relative path within the marketplace repo, e.g. "./plugins/my-plugin".
    Path(String),
    /// Legacy agent-skills marketplace format (source object tagged with "type").
    Legacy(LegacySource),
    /// Modern plugin marketplace format (source object tagged with "source").
    Modern(ModernSource),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum LegacySource {
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
#[serde(tag = "source")]
pub enum ModernSource {
    #[serde(rename = "github")]
    GitHub {
        repo: String,
        #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
        reference: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        sha: Option<String>,
    },
    #[serde(rename = "url")]
    Url {
        url: String,
        #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
        reference: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        sha: Option<String>,
    },
    #[serde(rename = "git-subdir")]
    GitSubdir {
        url: String,
        path: String,
        #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
        reference: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        sha: Option<String>,
    },
    #[serde(rename = "npm")]
    Npm {
        package: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        version: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        registry: Option<String>,
    },
    #[serde(rename = "archive")]
    Archive {
        url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        sha256: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub enum ResolvedSource {
    Path(String),
    Github {
        repo: String,
        reference: Option<String>,
        sha: Option<String>,
    },
    GitUrl {
        url: String,
        reference: Option<String>,
        sha: Option<String>,
    },
    GitSubdir {
        url: String,
        path: String,
        reference: Option<String>,
        sha: Option<String>,
    },
    Npm {
        package: String,
        version: Option<String>,
    },
    Pip {
        package: String,
    },
    Archive {
        url: String,
        sha256: Option<String>,
    },
}

impl MarketplaceSource {
    pub fn resolve(&self) -> ResolvedSource {
        match self {
            MarketplaceSource::Path(p) => ResolvedSource::Path(p.clone()),
            MarketplaceSource::Legacy(LegacySource::GitHub {
                repo,
                subdir,
                branch,
            }) => match subdir {
                Some(path) => ResolvedSource::GitSubdir {
                    url: repo.clone(),
                    path: path.clone(),
                    reference: branch.clone(),
                    sha: None,
                },
                None => ResolvedSource::Github {
                    repo: repo.clone(),
                    reference: branch.clone(),
                    sha: None,
                },
            },
            MarketplaceSource::Legacy(LegacySource::Url { url }) => ResolvedSource::GitUrl {
                url: url.clone(),
                reference: None,
                sha: None,
            },
            MarketplaceSource::Legacy(LegacySource::GitSubdir {
                repo,
                subdir,
                branch,
            }) => ResolvedSource::GitSubdir {
                url: repo.clone(),
                path: subdir.clone(),
                reference: branch.clone(),
                sha: None,
            },
            MarketplaceSource::Legacy(LegacySource::Npm { package }) => ResolvedSource::Npm {
                package: package.clone(),
                version: None,
            },
            MarketplaceSource::Legacy(LegacySource::Pip { package }) => ResolvedSource::Pip {
                package: package.clone(),
            },
            MarketplaceSource::Modern(ModernSource::GitHub {
                repo,
                reference,
                sha,
            }) => ResolvedSource::Github {
                repo: repo.clone(),
                reference: reference.clone(),
                sha: sha.clone(),
            },
            MarketplaceSource::Modern(ModernSource::Url {
                url,
                reference,
                sha,
            }) => ResolvedSource::GitUrl {
                url: url.clone(),
                reference: reference.clone(),
                sha: sha.clone(),
            },
            MarketplaceSource::Modern(ModernSource::GitSubdir {
                url,
                path,
                reference,
                sha,
            }) => ResolvedSource::GitSubdir {
                url: url.clone(),
                path: path.clone(),
                reference: reference.clone(),
                sha: sha.clone(),
            },
            MarketplaceSource::Modern(ModernSource::Npm {
                package,
                version,
                registry: _,
            }) => ResolvedSource::Npm {
                package: package.clone(),
                version: version.clone(),
            },
            MarketplaceSource::Modern(ModernSource::Archive { url, sha256 }) => {
                ResolvedSource::Archive {
                    url: url.clone(),
                    sha256: sha256.clone(),
                }
            }
        }
    }
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
        match serde_json::from_str::<Marketplace>(&content) {
            Ok(m) => Ok(m),
            Err(object_err) => {
                // Legacy format: a bare array of entries.
                match serde_json::from_str::<Vec<MarketplaceEntry>>(&content) {
                    Ok(entries) => Ok(Marketplace {
                        name: None,
                        owner: None,
                        metadata: None,
                        plugins: entries,
                    }),
                    Err(array_err) => Err(crate::error::Error::Conversion(format!(
                        "Failed to parse marketplace.json (tried object and array formats): {} / {}",
                        object_err, array_err
                    ))),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_modern_object_format() {
        let json = r#"{
            "name": "acme-tools",
            "owner": {"name": "Acme"},
            "plugins": [
                {"name": "formatter", "source": "./plugins/formatter"},
                {"name": "deploy", "source": {"source": "github", "repo": "acme/deploy-plugin"}}
            ]
        }"#;
        let market: Marketplace = serde_json::from_str(json).unwrap();
        assert_eq!(market.plugins.len(), 2);
        assert!(matches!(
            market.plugins[0].source,
            MarketplaceSource::Path(_)
        ));
        assert!(matches!(
            market.plugins[1].source,
            MarketplaceSource::Modern(ModernSource::GitHub { .. })
        ));
    }

    #[test]
    fn parse_legacy_array_format() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("repo");
        std::fs::create_dir_all(dir.join(".claude-plugin")).unwrap();
        std::fs::write(
            dir.join(".claude-plugin").join("marketplace.json"),
            r#"[{"name": "test", "source": {"type": "github", "repo": "user/repo"}}]"#,
        )
        .unwrap();
        let market = Marketplace::parse(&dir).unwrap();
        assert_eq!(market.plugins.len(), 1);
        assert!(matches!(
            market.plugins[0].source,
            MarketplaceSource::Legacy(LegacySource::GitHub { .. })
        ));
    }

    #[test]
    fn resolve_string_source_with_plugin_root() {
        let json = r#"{
            "metadata": {"pluginRoot": "./plugins"},
            "plugins": [{"name": "x", "source": "formatter"}]
        }"#;
        let market: Marketplace = serde_json::from_str(json).unwrap();
        let ResolvedSource::Path(p) = market.plugins[0].source.resolve() else {
            panic!("expected path source");
        };
        assert_eq!(p, "formatter");
        let root = market
            .metadata
            .as_ref()
            .and_then(|m| m.plugin_root.clone())
            .unwrap();
        assert_eq!(root, "./plugins");
    }

    #[test]
    fn resolve_modern_git_subdir() {
        let json = r#"{"plugins": [{"name": "x", "source": {"source": "git-subdir", "url": "https://github.com/acme/mono.git", "path": "tools/plugin", "ref": "v2.0.0"}}]}"#;
        let market: Marketplace = serde_json::from_str(json).unwrap();
        match market.plugins[0].source.resolve() {
            ResolvedSource::GitSubdir {
                url,
                path,
                reference,
                ..
            } => {
                assert_eq!(url, "https://github.com/acme/mono.git");
                assert_eq!(path, "tools/plugin");
                assert_eq!(reference.as_deref(), Some("v2.0.0"));
            }
            _ => panic!("expected git-subdir source"),
        }
    }
}
