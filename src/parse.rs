use std::{collections::HashMap, fmt::Display, sync::Arc};

use semver::VersionReq;
use tokio::sync::RwLock;
use tower_lsp::lsp_types::{Position, Range, Url};

#[derive(Debug, Clone)]
pub struct Dependency {
    pub name: String,
    pub version: Option<DependencyVersion>,
}

impl Display for Dependency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(version) = &self.version {
            write!(f, "{} = \"{}\"", self.name, version)
        } else {
            write!(f, "{} = \"?\"", self.name)
        }
    }
}

#[derive(Debug, Clone)]
pub enum DependencyVersion {
    Partial { range: Range, version: String },
    Complete { range: Range, version: VersionReq },
}

impl DependencyVersion {
    fn range_mut(&mut self) -> &mut Range {
        match self {
            DependencyVersion::Partial { range, .. }
            | DependencyVersion::Complete { range, .. } => range,
        }
    }
}

impl Display for DependencyVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DependencyVersion::Partial { version, .. } => f.write_str(&version),
            DependencyVersion::Complete { version, .. } => write!(f, "{}", version),
        }
    }
}

enum DocumentState {
    Start,
    Package,
    Dependencies,
    DevDependencies,
}

#[derive(Debug)]
enum Line<'a> {
    Start,
    PartialName {
        start: usize,
    },
    Name {
        name: &'a str,
    },
    Struct {
        name: &'a str,
        remainder: &'static str,
    },
    VersionSelector {
        name: &'a str,
        start: usize,
        first: bool,
    },
    Complete {
        name: &'a str,
        start: usize,
        end: usize,
        version: Option<&'a str>,
    },
    Partial {
        name: &'a str,
        start: usize,
        version: Option<&'a str>,
    },
}

impl<'a> Line<'a> {
    pub fn parse(line: &'a str) -> Option<Dependency> {
        use Line::*;
        let mut state = Start;

        for (i, c) in line.chars().enumerate() {
            state = match state {
                Complete { .. } | Partial { .. } => break,
                Start => {
                    if c.is_alphabetic() {
                        PartialName { start: i }
                    } else {
                        return None;
                    }
                }
                PartialName { start } => match c {
                    '-' | '_' => state,
                    c if c.is_alphanumeric() => state,
                    _ => Name {
                        name: &line[start..i],
                    },
                },
                Name { name } => match c {
                    '{' => Struct {
                        name,
                        remainder: "version",
                    },
                    '"' => VersionSelector {
                        name,
                        start: i,
                        first: true,
                    },
                    _ => Name { name },
                },
                Struct { name, remainder } => {
                    if remainder.is_empty() {
                        if c == '"' {
                            VersionSelector {
                                name,
                                start: i,
                                first: true,
                            }
                        } else {
                            Struct { name, remainder }
                        }
                    } else {
                        if let Some(remainder) = remainder.strip_prefix(c) {
                            Struct { name, remainder }
                        } else {
                            Struct {
                                name,
                                remainder: "version",
                            }
                        }
                    }
                }
                VersionSelector { name, start, first } => match c {
                    '"' => Complete {
                        name,
                        start,
                        end: i,
                        version: Some(&line[start..i]),
                    },
                    'a'..='z' | 'A'..='Z' => {
                        if first {
                            Partial {
                                name,
                                start,
                                version: Some(&line[start..i]),
                            }
                        } else {
                            VersionSelector {
                                name,
                                start,
                                first: false,
                            }
                        }
                    }
                    '0'..='9' | '.' | '_' | '<' | '>' | '=' | ',' => VersionSelector {
                        name,
                        start,
                        first: false,
                    },
                    ' ' => VersionSelector {
                        name,
                        start,
                        first: true,
                    },
                    _ => Partial {
                        name,
                        start,
                        version: Some(&line[start..i]),
                    },
                },
            };
        }

        match state {
            Complete {
                name,
                version,
                start,
                end,
            } => Some(Dependency {
                name: name.to_string(),
                version: if let Some(version) = version {
                    let version = version[1..].trim();

                    if let Ok(version) = VersionReq::parse(version) {
                        Some(DependencyVersion::Complete {
                            version,
                            range: Range::new(
                                Position::new(0, start as u32),
                                Position::new(0, end as u32),
                            ),
                        })
                    } else {
                        Some(DependencyVersion::Partial {
                            version: version.to_string(),
                            range: Range::new(
                                Position::new(0, start as u32),
                                Position::new(0, end as u32),
                            ),
                        })
                    }
                } else {
                    None
                },
            }),
            Partial {
                name,
                version,
                start,
            } => Some(Dependency {
                name: name.to_string(),
                version: if let Some(version) = version {
                    Some(DependencyVersion::Partial {
                        version: version[1..].trim().to_string(),
                        range: Range::new(
                            Position::new(0, start as u32),
                            Position::new(0, line.len() as u32),
                        ),
                    })
                } else {
                    None
                },
            }),
            Name { name, .. } | Struct { name, .. } => Some(Dependency {
                name: name.to_string(),
                version: None,
            }),
            VersionSelector { name, start, .. } => Some(Dependency {
                name: name.to_string(),
                version: Some(DependencyVersion::Partial {
                    version: line[start + 1..].trim().to_string(),
                    range: Range::new(
                        Position::new(0, start as u32),
                        Position::new(0, line.len() as u32),
                    ),
                }),
            }),
            _ => None,
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct ManifestTracker {
    manifests: Arc<RwLock<HashMap<Url, Vec<Dependency>>>>,
}

impl ManifestTracker {
    pub async fn update_from_source(&self, url: Url, source: &str) -> Vec<Dependency> {
        use DocumentState::*;
        let mut packages = Vec::new();

        let mut document = DocumentState::Start;

        for (i, line) in source.lines().enumerate() {
            if let Some(section) = match line.trim() {
                "[package]" => Some(DocumentState::Package),
                "[dependencies]" => Some(DocumentState::Dependencies),
                "[dev-dependencies]" => Some(DocumentState::DevDependencies),
                _ => None,
            } {
                document = section;
                continue;
            }

            if line.trim().is_empty() {
                continue;
            }

            match document {
                Start => (),
                Package => (),
                Dependencies | DevDependencies => {
                    if let Some(mut dependency) = Line::parse(line) {
                        // Line::parse assumes line 0, modify so we have to fix this manually.
                        if let Some(version) = dependency.version.as_mut() {
                            version.range_mut().start.line = i as u32;
                            version.range_mut().end.line = i as u32;
                        }
                        packages.push(dependency)
                    }
                }
            };
        }

        let mut lock = self.manifests.write().await;
        lock.insert(url, packages.clone());

        packages
    }

    async fn get(&self, url: &Url) -> Option<Vec<Dependency>> {
        let dependencies = {
            let lock = self.manifests.read().await;
            lock.get(url).cloned()
        };

        dependencies
    }
}

#[cfg(test)]
mod tests {
    use indoc::indoc;
    use tower_lsp::lsp_types::Url;

    use crate::parse::ManifestTracker;

    #[tokio::test]
    async fn detect_plain_version() {
        let url = Url::parse("file:///test").unwrap();

        let cargo = indoc! {r#"
            [dependencies]
            complete_simple_major = "1"
            complete_simple_minor = "1.2"
            complete_simple_patch = "1.2.3"
            complete_simple_range = ">=1, <2"
            complete = { version = "1.2.3" }
            partial_simple = "1.2
            partial_simple_pre = "1.2.0-alpha.1"
            partial_struct1 = { version = "1.20 }
            partial_struct2 = { version = "1.20
            partial_struct3 = { version = "1.20 features = ["serde"] }
            partial_struct4 = { version = "1.20 feature }
            partial_struct5 = { version = "1.20, features = }
        "#};

        let manifests = ManifestTracker::default();
        manifests.update_from_source(url.clone(), cargo).await;

        for dependency in manifests.get(&url).await.unwrap() {
            println!("{dependency}");
        }
    }
}
