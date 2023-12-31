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
        let dependency_names: Vec<&str> = packages
            .iter()
            .map(|dependency| dependency.name.as_str())
            .collect();

        // Get the newest version of each crate that appears in the manifest.
        let newest_packages = if self.settings.use_api().await {
            self.api
                .fetch_versions(self.cache.clone(), &dependency_names)
                .await
        } else {
            self.sparse
                .fetch_versions(self.cache.clone(), &dependency_names)
                .await
        };

        // Produce diagnostic hints for each crate where we might be helpful.
        let diagnostics: Vec<_> = packages
            .into_iter()
            .filter_map(|dependency| {
                if let Some(version) = dependency.version {
                    if let Some(Some(newest_version)) = newest_packages.get(&dependency.name) {
                        match version {
                            DependencyVersion::Complete { range, version } => {
                                if !version.matches(newest_version) {
                                    return Some(Diagnostic::new_simple(
                                        range,
                                        format!("{}: {newest_version}", &dependency.name),
                                    ));
                                } else {
                                    let range = Range {
                                        start: Position::new(range.start.line, 0),
                                        end: Position::new(range.start.line, 0),
                                    };

                                    return Some(Diagnostic::new_simple(range, "✓".to_string()));
                                }
                            }

                            DependencyVersion::Partial { range, .. } => {
                                return Some(Diagnostic::new_simple(
                                    range,
                                    format!("{}: {newest_version}", &dependency.name),
                                ));
                            }
                        }
                    } else {
                        return Some(Diagnostic::new_simple(
                            version.range(),
                            format!("{}: Unknown crate", &dependency.name),
                        ));
                    }
                }

                None
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

        let Some(dependency) = dependencies.into_iter().find(|dependency| {
            dependency.version.as_ref().is_some_and(|version| {
                version.range().start.line == cursor.line
                    && version.range().start.character <= cursor.character
                    && version.range().end.character >= cursor.character
            })
        }) else {
            return Ok(None);
        };

        let packages = self
            .sparse
            .fetch_versions(self.cache.clone(), &[&dependency.name])
            .await;

        if let Some(Some(newest_version)) = packages.get(&dependency.name) {
            let specified_version = dependency.version.as_ref().unwrap().to_string();
            let specified_version = &specified_version[0..specified_version.len() - 1];

            let newest_version = newest_version.to_string();

            let truncated_version = newest_version
                .as_str()
                .strip_prefix(
                    specified_version.trim_start_matches(&['<', '>', '=', '^', '~'] as &[_]),
                )
                .unwrap_or(&newest_version)
                .to_string();

            Ok(Some(CompletionResponse::Array(vec![CompletionItem {
                insert_text: Some(truncated_version),
                label: newest_version,

                ..CompletionItem::default()
            }])))
        } else {
            Ok(None)
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
