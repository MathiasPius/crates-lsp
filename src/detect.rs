use std::fmt::Display;

use semver::VersionReq;

#[derive(Debug, Clone)]
pub struct Dependency {
    pub name: String,
    pub version: Option<DependencyVersion>,
}

#[derive(Debug, Clone)]
pub enum DependencyVersion {
    Partial {
        start: usize,
        end: usize,
        version: String,
    },
    Complete {
        start: usize,
        end: usize,
        version: VersionReq,
    },
}

impl DependencyVersion {
    pub fn end(&self) -> usize {
        match self {
            DependencyVersion::Partial { end, .. } | DependencyVersion::Complete { end, .. } => {
                *end
            }
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
                            start,
                            end,
                        })
                    } else {
                        Some(DependencyVersion::Partial {
                            version: version.to_string(),
                            start,
                            end,
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
                        start,
                        end: line.len(),
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
                    start,
                    end: line.len(),
                }),
            }),
            _ => None,
        }
    }
}

pub fn detect_versions(source: &str) -> Vec<(usize, Dependency)> {
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
                if let Some(dependency) = Line::parse(line) {
                    packages.push((i, dependency))
                }
            }
        };
    }

    packages
}

#[cfg(test)]
mod tests {
    use super::detect_versions;
    use indoc::indoc;

    #[test]
    fn detect_plain_version() {
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

        let versions = detect_versions(&cargo);

        for (line, dep) in versions {
            println!("{line}: {dep:?}");
        }
    }
}
