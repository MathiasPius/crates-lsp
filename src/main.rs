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
            .clone()
            .into_iter()
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
        let diagnostics: Vec<_> = dependency_with_versions
            .into_iter()
            .map(|dependency| {
                if let Some(Some(newest_version)) = newest_packages.get(&dependency.name) {
                    match &dependency.version {
                        DependencyVersion::Complete { range, version } => {
                            if !version.matches(newest_version) {
                                Diagnostic::new(
                                    *range,
                                    Some(DiagnosticSeverity::INFORMATION),
                                    None,
                                    None,
                                    format!("{}: {newest_version}", &dependency.name),
                                    None,
                                    None,
                                )
                            } else {
                                let range = Range {
                                    start: Position::new(range.start.line, 0),
                                    end: Position::new(range.start.line, 0),
                                };
                                Diagnostic::new(
                                    range,
                                    Some(DiagnosticSeverity::HINT),
                                    None,
                                    None,
                                    "✓".to_string(),
                                    None,
                                    None,
                                )
                            }
                        }
                        DependencyVersion::Partial { range, .. } => Diagnostic::new(
                            *range,
                            Some(DiagnosticSeverity::INFORMATION),
                            None,
                            None,
                            format!("{}: {newest_version}", &dependency.name),
                            None,
                            None,
                        ),
                    }
                } else {
                    Diagnostic::new(
                        dependency.version.range(),
                        Some(DiagnosticSeverity::WARNING),
                        None,
                        None,
                        format!("{}: Unknown crate", &dependency.name),
                        None,
                        None,
                    )
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
            self.settings.populate_from(settings).await;
        }

        Ok(InitializeResult {
            server_info: None,
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
                    let specified_version = &specified_version[0..specified_version.len() - 1];

                    let newest_version = newest_version.to_string();

                    let truncated_version = newest_version
                        .as_str()
                        .strip_prefix(
                            specified_version
                                .trim_start_matches(&['<', '>', '=', '^', '~'] as &[_]),
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
