use std::{collections::HashMap, fmt::Display, sync::Arc};

use semver::VersionReq;
use tokio::sync::RwLock;
use tower_lsp::lsp_types::{Position, Range, Url};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dependency {
    /// e.g: anyho
    Partial {
        name: String,
        line: u32,
    },
    WithVersion(DependencyWithVersion),
    /// e.g: anyhow = { git = ".."}
    Other {
        name: String,
    },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyWithVersion {
    pub name: String,
    pub version: DependencyVersion,
}
// pub struct Dependency {
//     pub name: String,
//     pub version: Option<DependencyVersion>,
// }

impl Dependency {
    pub fn name(&self) -> Option<&String> {
        match self {
            Dependency::Partial { .. } => None,
            Dependency::WithVersion(dep) => Some(&dep.name),
            Dependency::Other { name } => Some(name),
        }
    }

    pub fn name_mut(&mut self) -> Option<&mut String> {
        match self {
            Dependency::Partial { .. } => None,
            Dependency::WithVersion(dep) => Some(&mut dep.name),
            Dependency::Other { name } => Some(name),
        }
    }

    pub fn version_mut(&mut self) -> Option<&mut DependencyVersion> {
        match self {
            Dependency::Partial { .. } => None,
            Dependency::WithVersion(dep) => Some(&mut dep.version),
            Dependency::Other { .. } => None,
        }
    }
}

impl Display for Dependency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Dependency::Partial { name, .. } => {
                write!(f, "{} = \"?\"", name)
            }
            Dependency::WithVersion(dep) => {
                write!(f, "{} = \"{}\"", dep.name, dep.version)
            }
            Dependency::Other { name } => {
                write!(f, "{} = \"?\"", name)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencyVersion {
    Partial { range: Range, version: String },
    Complete { range: Range, version: VersionReq },
}

impl DependencyVersion {
    pub fn range(&self) -> Range {
        match self {
            DependencyVersion::Partial { range, .. }
            | DependencyVersion::Complete { range, .. } => *range,
        }
    }

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
            DependencyVersion::Partial { version, .. } => f.write_str(version),
            DependencyVersion::Complete { version, .. } => write!(f, "{}", version),
        }
    }
}

enum DocumentState {
    Dependencies,
    Dependency(String),
    Other,
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
        version: &'a str,
    },
    Partial {
        name: &'a str,
        start: usize,
        version: &'a str,
    },
}

impl<'a> Line<'a> {
    pub fn parse(line: &'a str, line_no: usize) -> Option<Dependency> {
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
                    } else if let Some(remainder) = remainder.strip_prefix(c) {
                        Struct { name, remainder }
                    } else {
                        Struct {
                            name,
                            remainder: "version",
                        }
                    }
                }
                VersionSelector { name, start, first } => match c {
                    '"' => Complete {
                        name,
                        start,
                        end: i,
                        version: &line[start..i],
                    },
                    'a'..='z' | 'A'..='Z' => {
                        if first {
                            Partial {
                                name,
                                start,
                                version: &line[start..i],
                            }
                        } else {
                            VersionSelector {
                                name,
                                start,
                                first: false,
                            }
                        }
                    }
                    '0'..='9' | '.' | '_' | '-' | '<' | '>' | '=' | ',' => VersionSelector {
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
                        version: &line[start..i],
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
            } => {
                let version = version[1..].trim();
                let version = if let Ok(version) = VersionReq::parse(version) {
                    DependencyVersion::Complete {
                        version,
                        range: Range::new(
                            Position::new(0, start as u32),
                            Position::new(0, end as u32),
                        ),
                    }
                } else {
                    DependencyVersion::Partial {
                        version: version.to_string(),
                        range: Range::new(
                            Position::new(0, start as u32),
                            Position::new(0, end as u32),
                        ),
                    }
                };
                Some(Dependency::WithVersion(DependencyWithVersion {
                    name: name.to_string(),
                    version,
                }))
            }
            Partial {
                name,
                version,
                start,
            } => {
                let version = DependencyVersion::Partial {
                    version: version[1..].trim().trim_matches(',').to_string(),
                    range: Range::new(
                        Position::new(0, start as u32),
                        Position::new(0, line.len() as u32),
                    ),
                };
                Some(Dependency::WithVersion(DependencyWithVersion {
                    name: name.to_string(),
                    version,
                }))
            }
            Name { name, .. } | Struct { name, .. } => Some(Dependency::Other {
                name: name.to_string(),
            }),
            VersionSelector { name, start, .. } => {
                Some(Dependency::WithVersion(DependencyWithVersion {
                    name: name.to_string(),
                    version: DependencyVersion::Partial {
                        version: line[start + 1..].trim().to_string(),
                        range: Range::new(
                            Position::new(0, start as u32),
                            Position::new(0, line.len() as u32),
                        ),
                    },
                }))
            }
            PartialName { start } => Some(Dependency::Partial {
                name: line[start..].to_string(),
                line: line_no as u32,
            }),
            Start => None,
        }
        // very hacky should probably fix parsing
        .map(|mut d| match d {
            Dependency::WithVersion(ref mut v) => {
                let range = v.version.range_mut();
                if let Some('"') = line.chars().nth(range.start.character as usize) {
                    range.start.character += 1;
                }
                if let Some('"') = line.chars().nth(range.end.character as usize + 1) {
                    range.end.character -= 1;
                }
                d
            }
            _ => d,
        })
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

        // We use this to keep track of our current context within the document,
        // since we only want to act on dependencies in actual dependency sections,
        // and not pick up `version = "1.2.3"` as a dependency on a "version" crate
        // in the middle of the package section.
        let mut document = DocumentState::Other;

        for (i, line) in source.lines().enumerate() {
            let line = line.trim();

            if line.is_empty() {
                continue;
            }

            // Detect start of new section.
            if line.starts_with('[') {
                if line.starts_with("[dependencies") {
                    if let Some(package) = line.strip_prefix("[dependencies.") {
                        // This is the case where a dependency is specified over multiple lines, for example:
                        //
                        // ```toml
                        // [dependencies.serde]
                        // version = "1.0.108"
                        // ```
                        document =
                            DocumentState::Dependency(package.trim_end_matches(']').to_string());
                    } else {
                        // This is just a plain old [dependencies] section
                        document = DocumentState::Dependencies;
                    }
                } else if line.ends_with("dependencies]") {
                    // Covers [build-dependencies], [dev-dependencies], [target.'cfg(unix)'.dependencies], etc.
                    // Crucially does *not* break specifying packages ending in "dependencies" in the verbose way
                    // since that case is covered by the previous if-branch matching on '[dependencies':
                    //
                    // ```toml
                    // [dependencies.crate-ending-in-dependencies]
                    // version = "1"
                    // ```
                    document = DocumentState::Dependencies;
                } else {
                    document = DocumentState::Other;
                }

                // Section starts cannot contain version information, so skip the rest of the loop.
                continue;
            }

            match document {
                Dependencies => {
                    // If we're in a generic dependency section, and find a line
                    // which can be parsed as a versioned dependency, push it as a package.
                    if let Some(mut dependency) = Line::parse(line, i) {
                        // Line::parse assumes line 0, modify so we have to fix this manually.
                        if let Some(version) = dependency.version_mut() {
                            version.range_mut().start.line = i as u32;
                            version.range_mut().end.line = i as u32;
                        }
                        packages.push(dependency)
                    }
                }
                Dependency(ref name) => {
                    // We parse the line as a regular dependency, and check if the dependency name is "version"
                    // This is a hack, but it means we don't have to write custom parsing code for sections like this:

                    // ```toml
                    // [dependencies.serde]
                    // version = "1"
                    // ```
                    if let Some(mut dependency) = Line::parse(line, i) {
                        if dependency
                            .name()
                            .map(|x| x != "version")
                            .unwrap_or_default()
                        {
                            continue;
                        } else {
                            // Rename to the package section, since the dependency is currently
                            // named "version" because of the Line::parse logic assuming this is
                            // a regular dependencies section.
                            if let Some(x) = dependency.name_mut() {
                                x.clone_from(name)
                            }
                        }
                        // Line::parse assumes line 0, modify so we have to fix this manually.
                        if let Some(version) = dependency.version_mut() {
                            version.range_mut().start.line = i as u32;
                            version.range_mut().end.line = i as u32;
                        }
                        packages.push(dependency)
                    }
                }
                // We're either at the start of the document, or in an irrelevant section
                // such as [package], do nothing.
                Other => (),
            };
        }

        let mut lock = self.manifests.write().await;
        lock.insert(url, packages.clone());

        packages
    }

    pub async fn get(&self, url: &Url) -> Option<Vec<Dependency>> {
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
    use semver::VersionReq;
    use tower_lsp::lsp_types::Position;
    use tower_lsp::lsp_types::Range;
    use tower_lsp::lsp_types::Url;

    use crate::parse::DependencyVersion;
    use crate::parse::Line;
    use crate::parse::ManifestTracker;
    use crate::parse::{Dependency, DependencyWithVersion};

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

    fn matches_complete(line: &str, name: &str, version: &str) {
        let line = Line::parse(line, 0).unwrap();
        let Dependency::WithVersion(line) = line else {
            panic!("expected complete version selector")
        };
        let expected_version = VersionReq::parse(version).unwrap();

        assert_eq!(line.name, name);

        match line.version {
            DependencyVersion::Partial { .. } => panic!("expected complete version selector"),
            DependencyVersion::Complete { version, .. } => {
                assert_eq!(version, expected_version)
            }
        }
    }

    fn matches_partial(line: &str, name: &str, expected_version: &str) {
        let line = Line::parse(line, 0).unwrap();
        let Dependency::WithVersion(line) = line else {
            panic!("expected complete version selector")
        };
        assert_eq!(line.name, name);

        match line.version {
            DependencyVersion::Complete { .. } => panic!("expected partial version selector"),
            DependencyVersion::Partial { version, .. } => {
                assert_eq!(version.as_str(), expected_version)
            }
        }
    }

    #[test]
    fn parse_complete() {
        matches_complete("complete = \"1.2.3\"", "complete", "1.2.3");
        matches_complete("complete = \"=1.2.3\"", "complete", "=1.2.3");
        matches_complete("complete = \"1.2\"", "complete", "1.2");
        matches_complete("complete = \"=1.2\"", "complete", "=1.2");
        matches_complete("complete = \"1\"", "complete", "1");
        matches_complete("complete = \"=1\"", "complete", "=1");
    }

    #[test]
    fn parse_complete_version_field() {
        matches_complete("complete = { version = \"1.2.3\" }", "complete", "1.2.3");
        matches_complete("complete = { version = \"=1.2.3\" }", "complete", "=1.2.3");
        matches_complete("complete = { version = \"1.2\" }", "complete", "1.2");
        matches_complete("complete = { version = \"=1.2\" }", "complete", "=1.2");
        matches_complete("complete = { version = \"1\" }", "complete", "1");
        matches_complete("complete = { version = \"=1\" }", "complete", "=1");
    }

    #[test]
    fn parse_partial() {
        matches_partial("partial = \"1.2.3", "partial", "1.2.3");
        matches_partial("partial = \"1.2.", "partial", "1.2.");
        matches_partial("partial = \"1.2", "partial", "1.2");
        matches_partial("partial \"1.", "partial", "1.");
        matches_partial("partial \"1", "partial", "1");

        matches_partial("partial \"1.2.3, features = [", "partial", "1.2.3");
        matches_partial("partial \"1.2., features = [", "partial", "1.2.");
        matches_partial("partial \"1.2, features = [", "partial", "1.2");
        matches_partial("partial \"1., features = [", "partial", "1.");
        matches_partial("partial \"1, features = [", "partial", "1");
    }

    #[tokio::test]
    async fn parse_independent_dependency_section() {
        let url = Url::parse("file:///test").unwrap();

        let cargo = indoc! {r#"
            [dependencies]
            log = "1"
            
            [dependencies.serde]
            version = "1"
            
            [dependencies.tokio]
            version = "1"
        "#};

        let manifests = ManifestTracker::default();
        manifests.update_from_source(url.clone(), cargo).await;

        assert_eq!(
            manifests.get(&url).await.unwrap(),
            vec![
                Dependency::WithVersion(DependencyWithVersion {
                    name: "log".to_string(),
                    version: DependencyVersion::Complete {
                        range: Range {
                            start: Position::new(1, 7),
                            end: Position::new(1, 8)
                        },
                        version: VersionReq::parse("1").unwrap()
                    }
                }),
                Dependency::WithVersion(DependencyWithVersion {
                    name: "serde".to_string(),
                    version: DependencyVersion::Complete {
                        range: Range {
                            start: Position::new(4, 11),
                            end: Position::new(4, 12)
                        },
                        version: VersionReq::parse("1").unwrap()
                    }
                }),
                Dependency::WithVersion(DependencyWithVersion {
                    name: "tokio".to_string(),
                    version: DependencyVersion::Complete {
                        range: Range {
                            start: Position::new(7, 11),
                            end: Position::new(7, 12)
                        },
                        version: VersionReq::parse("1").unwrap()
                    }
                })
            ]
        );
    }
}
