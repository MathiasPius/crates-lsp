use std::sync::Arc;

use reqwest::Url;
use serde::Deserialize;
use tokio::sync::RwLock;
use tower_lsp::lsp_types::DiagnosticSeverity;

#[derive(Debug, Clone)]
pub struct Settings {
    inner: Arc<RwLock<LspSettings>>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            inner: Arc::new(RwLock::new(LspSettings {
                files: default_files(),
                ..Default::default()
            })),
        }
    }
}

impl Settings {
    pub async fn populate_from(
        &self,
        value: serde_json::Value,
    ) -> Result<LspSettings, serde_json::Error> {
        let new_settings: LspSettings = serde_json::from_value(value)?;

        let mut internal_settings = self.inner.write().await;
        *internal_settings = new_settings.clone();

        Ok(new_settings)
    }

    pub async fn use_api(&self) -> bool {
        self.inner.read().await.use_api.unwrap_or_default()
    }

    pub async fn inlay_hints(&self) -> bool {
        self.inner.read().await.inlay_hints.unwrap_or(true)
    }

    pub async fn diagnostics(&self) -> bool {
        self.inner.read().await.diagnostics.unwrap_or(true)
    }

    pub async fn needs_update_severity(&self) -> DiagnosticSeverity {
        self.inner
            .read()
            .await
            .needs_update_severity
            .filter(verify_severity)
            .unwrap_or(DiagnosticSeverity::INFORMATION)
    }

    pub async fn up_to_date_severity(&self) -> DiagnosticSeverity {
        self.inner
            .read()
            .await
            .up_to_date_severity
            .filter(verify_severity)
            .unwrap_or(DiagnosticSeverity::HINT)
    }

    pub async fn unknown_dep_severity(&self) -> DiagnosticSeverity {
        self.inner
            .read()
            .await
            .unknown_dep_severity
            .filter(verify_severity)
            .unwrap_or(DiagnosticSeverity::WARNING)
    }

    pub async fn up_to_date_hint(&self) -> String {
        self.inner
            .read()
            .await
            .up_to_date_hint
            .clone()
            .unwrap_or_else(|| "✓".to_string())
    }

    pub async fn needs_update_hint(&self) -> String {
        self.inner
            .read()
            .await
            .needs_update_hint
            .clone()
            .unwrap_or_else(|| " {}".to_string())
    }

    pub async fn matches_filename(&self, uri: &Url) -> bool {
        let Some(filename) = uri
            .path_segments()
            .and_then(|mut segments| segments.next_back())
        else {
            return false;
        };

        self.inner
            .read()
            .await
            .files
            .iter()
            .any(|matched_filename| matched_filename.as_str() == filename)
    }
}

// verify the config is a valid severity level
fn verify_severity(d: &DiagnosticSeverity) -> bool {
    *d >= DiagnosticSeverity::ERROR && *d <= DiagnosticSeverity::HINT
}

fn default_files() -> Vec<String> {
    vec!["Cargo.toml".to_string()]
}

#[derive(Default, Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LspSettings {
    #[serde(default)]
    pub use_api: Option<bool>,
    #[serde(default)]
    pub inlay_hints: Option<bool>,
    #[serde(default)]
    pub diagnostics: Option<bool>,
    #[serde(default)]
    pub needs_update_severity: Option<DiagnosticSeverity>,
    #[serde(default)]
    pub up_to_date_severity: Option<DiagnosticSeverity>,
    #[serde(default)]
    pub unknown_dep_severity: Option<DiagnosticSeverity>,
    #[serde(default)]
    pub up_to_date_hint: Option<String>,
    #[serde(default)]
    pub needs_update_hint: Option<String>,
    #[serde(default = "default_files")]
    pub files: Vec<String>,
}
