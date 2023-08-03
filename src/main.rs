use parse::{DependencyVersion, ManifestTracker};
use registry::api::CrateApi;
use registry::CrateRegistry;
use semver::Version;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

mod parse;
mod registry;

#[derive(Debug, Clone)]
struct Backend {
    client: Client,
    manifests: ManifestTracker,
    registry: CrateApi,
}

fn diagnostic_from_version(range: Range, version: &Version) -> Diagnostic {
    Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::INFORMATION),
        code: None,
        code_description: None,
        source: Some(String::from("crates-lsp")),
        message: version.to_string(),
        related_information: None,
        tags: None,
        data: None,
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: None,
            capabilities: ServerCapabilities {
                inlay_hint_provider: Some(OneOf::Left(true)),
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
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_change_watched_files(&self, _: DidChangeWatchedFilesParams) {
        self.client
            .log_message(MessageType::INFO, "watched files have changed!")
            .await;
    }

    async fn did_open(&self, _: DidOpenTextDocumentParams) {
        self.client
            .log_message(MessageType::INFO, "file opened!")
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        self.client
            .log_message(MessageType::INFO, "file changed!")
            .await;

        if let Some(content) = params.content_changes.first() {
            // Fetch the parsed manifest of the file in question.
            let packages = self
                .manifests
                .update_from_source(params.text_document.uri.clone(), &content.text)
                .await;

            // Retrieve just the package names, so we can fetch the latest
            // versions via the crate registry.
            let dependency_names: Vec<&str> = packages
                .iter()
                .map(|dependency| dependency.name.as_str())
                .collect();

            // Get the newest version of each crate that appears in the manifest.
            let newest_packages = self.registry.fetch_versions(&dependency_names).await;

            // Produce diagnostic hints for each crate where we might be helpful.
            let diagnostics: Vec<_> = packages
                .into_iter()
                .filter_map(|dependency| {
                    if let Some(version) = dependency.version {
                        if let Some(Some(newest_version)) = newest_packages.get(&dependency.name) {
                            match version {
                                DependencyVersion::Complete { range, version } => {
                                    if !version.matches(newest_version) {
                                        return Some(diagnostic_from_version(
                                            range,
                                            newest_version,
                                        ));
                                    }
                                }

                                DependencyVersion::Partial { range, .. } => {
                                    return Some(diagnostic_from_version(range, newest_version));
                                }
                            }
                        }
                    }

                    None
                })
                .collect();

            self.client
                .publish_diagnostics(
                    params.text_document.uri,
                    diagnostics,
                    Some(params.text_document.version),
                )
                .await;
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        self.client
            .log_message(MessageType::INFO, "file saved!")
            .await;

        if let Some(text) = params.text.as_deref() {
            self.manifests
                .update_from_source(params.text_document.uri, text)
                .await;
        }
    }

    async fn did_close(&self, _: DidCloseTextDocumentParams) {
        self.client
            .log_message(MessageType::INFO, "file closed!")
            .await;
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let cursor = params.text_document_position.position;

        let Some(dependencies) = self.manifests.get(&params.text_document_position.text_document.uri).await else {
            return Ok(None)
        };

        let Some(dependency) = dependencies.into_iter().find(|dependency| {
            dependency.version.as_ref().is_some_and(|version| {
                version.range().start.line == cursor.line && version.range().start.character <= cursor.character && version.range().end.character >= cursor.character
            })
        }) else {
            return Ok(None)
        };

        let packages = self.registry.fetch_versions(&[&dependency.name]).await;

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

    async fn inlay_hint(
        &self,
        _: tower_lsp::lsp_types::InlayHintParams,
    ) -> Result<Option<Vec<InlayHint>>> {
        Ok(Some(vec![]))
    }
    /*
    async fn inlay_hint(
        &self,
        params: tower_lsp::lsp_types::InlayHintParams,
    ) -> Result<Option<Vec<InlayHint>>> {
        let uri = &params.text_document.uri;

        self.client
            .log_message(
                MessageType::INFO,
                format!("inlay hint: {uri}, {:#?}", &self.versions),
            )
            .await;

        let versions = {
            let guard = self.versions.inner.read().await;
            guard.get(&params.text_document.uri).cloned()
        };

        let Some(versions) = versions else {
            return Ok(None);
        };

        self.client
            .log_message(
                MessageType::INFO,
                format!("inlay hint: {uri}, {}", versions.len()),
            )
            .await;

        Ok(Some(
            versions
                .iter()
                .filter_map(|dependency| {
                    let Some(ref version) = dependency.version else {
                        return None;
                    };

                    Some(InlayHint {
                        text_edits: None,
                        tooltip: None,
                        kind: Some(InlayHintKind::TYPE),
                        padding_left: None,
                        padding_right: None,
                        data: None,
                        position: version.range().end,
                        label: InlayHintLabel::LabelParts(vec![InlayHintLabelPart {
                            value: version.to_string(),
                            tooltip: None,
                            location: Some(Location {
                                uri: params.text_document.uri.clone(),
                                range: Range {
                                    start: Position::new(0, 4),
                                    end: Position::new(0, 5),
                                },
                            }),
                            command: None,
                        }]),
                    })
                })
                .collect(),
        ))
    }
    */

    /*
    async fn did_change_workspace_folders(&self, _: DidChangeWorkspaceFoldersParams) {
        self.client
            .log_message(MessageType::INFO, "workspace folders changed!")
            .await;
    }

    async fn did_change_configuration(&self, _: DidChangeConfigurationParams) {
        self.client
            .log_message(MessageType::INFO, "configuration changed!")
            .await;
    }

    async fn execute_command(&self, _: ExecuteCommandParams) -> Result<Option<Value>> {
        self.client
            .log_message(MessageType::INFO, "command executed!")
            .await;

        match self.client.apply_edit(WorkspaceEdit::default()).await {
            Ok(res) if res.applied => self.client.log_message(MessageType::INFO, "applied").await,
            Ok(_) => self.client.log_message(MessageType::INFO, "rejected").await,
            Err(err) => self.client.log_message(MessageType::ERROR, err).await,
        }

        Ok(None)
    }
    */
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    tracing_subscriber::fmt().init();

    let (stdin, stdout) = (tokio::io::stdin(), tokio::io::stdout());

    let (service, socket) = LspService::new(|client| Backend {
        client,
        manifests: ManifestTracker::default(),
        registry: CrateApi::default(),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
