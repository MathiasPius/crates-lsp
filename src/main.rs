use crate::parse::{Dependency, DependencyWithVersion};
use crates::api::CrateApi;
use crates::cache::CrateCache;
use crates::sparse::CrateIndex;
use crates::CrateLookup;
use parse::{DependencyVersion, ManifestTracker};
use settings::Settings;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

mod crates;
mod parse;
mod settings;

mod diagnostic_codes {
    pub const UP_TO_DATE: i32 = 0;
    pub const NEEDS_UPDATE: i32 = 1;
    pub const UNKNOWN_DEP: i32 = 2;
}

#[derive(Debug, Clone)]
struct Backend {
    client: Client,
    settings: Settings,
    manifests: ManifestTracker,
    api: CrateApi,
    sparse: CrateIndex,
    cache: CrateCache,
}

impl Backend {
    async fn calculate_diagnostics(&self, url: Url, content: &str) -> Vec<Diagnostic> {
        if !self.settings.matches_filename(&url).await {
            return Vec::new();
        }

        if !self.settings.diagnostics().await {
            return Vec::new();
        }

        let packages = self.manifests.update_from_source(url, content).await;

        // Retrieve just the package names, so we can fetch the latest
        // versions via the crate registry.
        let dependency_with_versions: Vec<&DependencyWithVersion> = packages
            .iter()
            .filter_map(|dependency| match dependency {
                Dependency::Partial { .. } => None,
                Dependency::WithVersion(dep) => Some(dep),
                Dependency::Other { .. } => None,
            })
            .collect();

        if dependency_with_versions.is_empty() {
            return Vec::new();
        }

        let crate_names: Vec<&str> = dependency_with_versions
            .iter()
            .map(|x| x.name.as_str())
            .collect();
        // Get the newest version of each crate that appears in the manifest.
        let newest_packages = if self.settings.use_api().await {
            self.api
                .fetch_versions(self.cache.clone(), &crate_names)
                .await
        } else {
            self.sparse
                .fetch_versions(self.cache.clone(), &crate_names)
                .await
        };

        // Produce diagnostic hints for each crate where we might be helpful.
        let nu_sev = self.settings.needs_update_severity().await;
        let utd_sev = self.settings.up_to_date_severity().await;
        let ud_sev = self.settings.unknown_dep_severity().await;
        let diagnostics: Vec<_> = dependency_with_versions
            .into_iter()
            .map(|dependency| {
                if let Some(Some(newest_version)) = newest_packages.get(&dependency.name) {
                    match &dependency.version {
                        DependencyVersion::Complete { range, version } => {
                            if !version.matches(newest_version) {
                                Diagnostic {
                                    range: *range,
                                    severity: Some(nu_sev),
                                    code: Some(NumberOrString::Number(
                                        diagnostic_codes::NEEDS_UPDATE,
                                    )),
                                    code_description: None,
                                    source: None,
                                    message: format!("{}: {newest_version}", &dependency.name),
                                    related_information: None,
                                    tags: None,
                                    data: Some(serde_json::json!({
                                        "newest_version": newest_version,
                                    })),
                                }
                            } else {
                                let range = Range {
                                    start: Position::new(range.start.line, 0),
                                    end: Position::new(range.start.line, 0),
                                };
                                Diagnostic::new(
                                    range,
                                    Some(utd_sev),
                                    Some(NumberOrString::Number(diagnostic_codes::UP_TO_DATE)),
                                    None,
                                    "✓".to_string(),
                                    None,
                                    None,
                                )
                            }
                        }
                        DependencyVersion::Partial { range, .. } => Diagnostic {
                            range: *range,
                            severity: Some(nu_sev),
                            code: Some(NumberOrString::Number(diagnostic_codes::NEEDS_UPDATE)),
                            code_description: None,
                            source: None,
                            message: format!("{}: {newest_version}", &dependency.name),
                            related_information: None,
                            tags: None,
                            data: Some(serde_json::json!({
                                "newest_version": newest_version,
                            })),
                        },
                    }
                } else {
                    Diagnostic {
                        range: dependency.version.range(),
                        severity: Some(ud_sev),
                        code: Some(NumberOrString::Number(diagnostic_codes::UNKNOWN_DEP)),
                        code_description: None,
                        source: None,
                        message: format!("{}: Unknown crate", &dependency.name),
                        related_information: None,
                        tags: None,
                        data: None,
                    }
                }
            })
            .collect();

        diagnostics
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        if let Some(settings) = params.initialization_options {
            match self.settings.populate_from(settings).await {
                Ok(settings) => {
                    self.client
                        .log_message(
                            MessageType::INFO,
                            format!("crates-lsp configured with settings: {settings:?}"),
                        )
                        .await
                }
                Err(err) => {
                    self.client
                        .log_message(
                            MessageType::ERROR,
                            format!("crates-lsp settings could not be parsed: {err:?}"),
                        )
                        .await
                }
            }
        }

        // Replace the cache directory with the
        self.cache
            .change_directory(self.settings.cache_directory().await)
            .await;

        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "crates-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: Some(vec![
                        "=".to_string(),
                        ".".to_string(),
                        "\"".to_string(),
                    ]),
                    work_done_progress_options: Default::default(),
                    all_commit_characters: None,
                    ..Default::default()
                }),
                inlay_hint_provider: Some(OneOf::Left(true)),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec!["dummy.do_something".to_string()],
                    work_done_progress_options: Default::default(),
                }),
                workspace: Some(WorkspaceServerCapabilities {
                    workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                        supported: Some(true),
                        change_notifications: Some(OneOf::Left(true)),
                    }),
                    file_operations: None,
                }),

                ..ServerCapabilities::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "crates-lsp initialized.")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if !self
            .settings
            .matches_filename(&params.text_document.uri)
            .await
        {
            return;
        }

        if let Some(content) = params.content_changes.first() {
            let diagnostics = self
                .calculate_diagnostics(params.text_document.uri.clone(), &content.text)
                .await;

            self.client
                .publish_diagnostics(
                    params.text_document.uri,
                    diagnostics,
                    Some(params.text_document.version),
                )
                .await;
        }
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        if !self
            .settings
            .matches_filename(&params.text_document.uri)
            .await
        {
            return;
        }

        let diagnostics = self
            .calculate_diagnostics(params.text_document.uri.clone(), &params.text_document.text)
            .await;

        self.client
            .publish_diagnostics(
                params.text_document.uri,
                diagnostics,
                Some(params.text_document.version),
            )
            .await;
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        if !self
            .settings
            .matches_filename(&params.text_document_position.text_document.uri)
            .await
        {
            return Ok(None);
        }

        let cursor = params.text_document_position.position;

        let Some(dependencies) = self
            .manifests
            .get(&params.text_document_position.text_document.uri)
            .await
        else {
            return Ok(None);
        };

        let Some(dependency) = dependencies
            .into_iter()
            .find(|dependency| match dependency {
                Dependency::Partial { line, .. } => *line == cursor.line,
                Dependency::WithVersion(dep) => {
                    dep.version.range().start.line == cursor.line
                        && dep.version.range().start.character <= cursor.character
                        && dep.version.range().end.character >= cursor.character
                }
                Dependency::Other { .. } => false,
            })
        else {
            return Ok(None);
        };

        match dependency {
            Dependency::Partial { name, .. } => {
                let Ok(crates) = self.sparse.search_crates(&name).await else {
                    return Ok(None);
                };
                let range = Range::new(Position::new(cursor.line, 0), cursor);
                Ok(Some(CompletionResponse::Array(
                    crates
                        .into_iter()
                        .map(|x| CompletionItem {
                            text_edit: Some(CompletionTextEdit::Edit(TextEdit::new(
                                range,
                                x.name.clone(),
                            ))),
                            label: x.name,
                            ..CompletionItem::default()
                        })
                        .collect(),
                )))
            }
            Dependency::WithVersion(dependency) => {
                let packages = self
                    .sparse
                    .fetch_versions(self.cache.clone(), &[&dependency.name])
                    .await;

                if let Some(Some(newest_version)) = packages.get(&dependency.name) {
                    let specified_version = dependency.version.to_string();

                    let newest_version = newest_version.to_string();

                    let truncated_version = newest_version
                        .as_str()
                        .strip_prefix(
                            specified_version.trim_start_matches(['<', '>', '=', '^', '~']),
                        )
                        .unwrap_or(&newest_version)
                        .to_string();

                    Ok(Some(CompletionResponse::Array(vec![CompletionItem {
                        insert_text: Some(truncated_version.clone()),
                        label: newest_version.clone(),

                        ..CompletionItem::default()
                    }])))
                } else {
                    Ok(None)
                }
            }
            Dependency::Other { .. } => {
                return Ok(None);
            }
        }
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        if !self
            .settings
            .matches_filename(&params.text_document.uri)
            .await
        {
            return Ok(None);
        }

        if !self.settings.inlay_hints().await {
            return Ok(None);
        }

        let utd_hint = self.settings.up_to_date_hint().await;
        let nu_hint = self.settings.needs_update_hint().await;

        if utd_hint.is_empty() && nu_hint.is_empty() {
            return Ok(None);
        }

        let Some(dependencies) = self.manifests.get(&params.text_document.uri).await else {
            return Ok(None);
        };
        let dependencies_with_versions: Vec<DependencyWithVersion> = dependencies
            .into_iter()
            .filter_map(|d| match d {
                Dependency::WithVersion(v) => (v.version.range().start >= params.range.start
                    && v.version.range().end <= params.range.end)
                    .then_some(v),
                Dependency::Other { .. } | Dependency::Partial { .. } => None,
            })
            .collect();

        if dependencies_with_versions.is_empty() {
            return Ok(None);
        }

        let crate_names: Vec<&str> = dependencies_with_versions
            .iter()
            .map(|x| x.name.as_str())
            .collect();

        let newest_packages = if self.settings.use_api().await {
            self.api
                .fetch_versions(self.cache.clone(), &crate_names)
                .await
        } else {
            self.sparse
                .fetch_versions(self.cache.clone(), &crate_names)
                .await
        };

        let mut v = if utd_hint.is_empty() || nu_hint.is_empty() {
            Vec::new() // if either is empty we dont know how many elements there are
        } else {
            Vec::with_capacity(dependencies_with_versions.len())
        };

        for dep in dependencies_with_versions {
            let Some(Some(newest_version)) = newest_packages.get(&dep.name) else {
                continue;
            };
            let (hint, tip, pos) = match dep.version {
                DependencyVersion::Complete { range, version } => {
                    let (hint, tip) = if version.matches(newest_version) {
                        if utd_hint.is_empty() {
                            continue;
                        }
                        (
                            utd_hint.replace("{}", &version.to_string()),
                            "up to date".to_string(),
                        )
                    } else {
                        if nu_hint.is_empty() {
                            continue;
                        }
                        (
                            nu_hint.replace("{}", &newest_version.to_string()),
                            "latest stable version".to_string(),
                        )
                    };
                    (
                        hint,
                        tip,
                        Position::new(range.end.line, range.end.character + 1),
                    )
                }
                DependencyVersion::Partial { range, .. } => {
                    if nu_hint.is_empty() {
                        continue;
                    }
                    (
                        nu_hint.replace("{}", &newest_version.to_string()),
                        "latest stable version".to_string(),
                        Position::new(range.end.line, range.end.character + 1),
                    )
                }
            };
            v.push(InlayHint {
                position: pos,
                label: InlayHintLabel::String(hint),
                kind: None,
                text_edits: None,
                tooltip: Some(InlayHintTooltip::String(tip)),
                padding_left: Some(true),
                padding_right: None,
                data: None,
            });
        }
        Ok(Some(v))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        if !self
            .settings
            .matches_filename(&params.text_document.uri)
            .await
        {
            return Ok(None);
        }

        let mut response = CodeActionResponse::new();
        for d in params
            .context
            .diagnostics
            .into_iter()
            .filter(|d| d.range.start <= params.range.start && d.range.end >= params.range.end)
        {
            let Some(NumberOrString::Number(diagnostic_codes::NEEDS_UPDATE)) = d.code else {
                continue;
            };

            let Some(serde_json::Value::Object(ref data)) = d.data else {
                continue;
            };

            let Some(serde_json::Value::String(newest_version)) = data.get("newest_version") else {
                continue;
            };

            let range = d.range;
            let newest_version = newest_version.clone();

            response.push(CodeActionOrCommand::CodeAction(CodeAction {
                title: format!("Update Version to: {newest_version}"),
                kind: Some(CodeActionKind::QUICKFIX),
                diagnostics: Some(vec![d]),
                edit: Some(WorkspaceEdit {
                    changes: Some(
                        [(
                            params.text_document.uri.clone(),
                            vec![TextEdit {
                                range,
                                new_text: newest_version,
                            }],
                        )]
                        .into(),
                    ),
                    document_changes: None,
                    change_annotations: None,
                }),
                command: None,
                is_preferred: None,
                disabled: None,
                data: None,
            }))
        }
        Ok(Some(response))
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let (stdin, stdout) = (tokio::io::stdin(), tokio::io::stdout());

    let (service, socket) = LspService::new(|client| Backend {
        client,
        manifests: ManifestTracker::default(),
        settings: Settings::default(),
        sparse: CrateIndex::default(),
        api: CrateApi::default(),
        cache: CrateCache::default(),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
