use parse::ManifestTracker;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

mod parse;
mod registry;

#[derive(Debug)]
struct Backend {
    client: Client,
    manifests: ManifestTracker,
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
                    trigger_characters: Some(vec!["=".to_string()]),
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
            self.manifests
                .update_from_source(params.text_document.uri, &content.text)
                .await;

            /*
            let dependencies = detect_versions(&content.text);

            self.client
                .publish_diagnostics(
                    params.text_document.uri.clone(),
                    dependencies
                        .iter()
                        .filter_map(|dependency| {
                            if let Some(DependencyVersion::Complete { range, .. }) =
                                dependency.version
                            {
                                Some(Diagnostic::new_simple(range, "Hello".to_string()))
                            } else {
                                None
                            }
                        })
                        .collect(),
                    Some(params.text_document.version),
                )
                .await;
            */
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

    async fn completion(&self, _: CompletionParams) -> Result<Option<CompletionResponse>> {
        Ok(Some(CompletionResponse::Array(vec![
            CompletionItem::new_simple("Hello".to_string(), "Some detail".to_string()),
            CompletionItem::new_simple("Bye".to_string(), "More detail".to_string()),
        ])))
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
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
