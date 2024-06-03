use std::sync::Arc;

use serde::Deserialize;
use tokio::sync::RwLock;
use tower_lsp::lsp_types::DiagnosticSeverity;

#[derive(Default, Debug, Clone)]
pub struct Settings {
    inner: Arc<RwLock<InnerSettings>>,
}

impl Settings {
    pub async fn populate_from(&self, value: serde_json::Value) {
        if let Ok(new_settings) = serde_json::from_value(value) {
            let mut internal_settings = self.inner.write().await;
            *internal_settings = new_settings;
        }
    }

    pub async fn use_api(&self) -> bool {
        self.inner.read().await.lsp.use_api.unwrap_or_default()
    }

    pub async fn needs_update_severity(&self) -> DiagnosticSeverity {
        self.inner
            .read()
            .await
            .lsp
            .needs_update_severity
            .filter(verify_severity)
            .unwrap_or(DiagnosticSeverity::INFORMATION)
    }

    pub async fn up_to_date_severity(&self) -> DiagnosticSeverity {
        self.inner
            .read()
            .await
            .lsp
            .up_to_date_severity
            .filter(verify_severity)
            .unwrap_or(DiagnosticSeverity::HINT)
    }

    pub async fn unknown_dep_severity(&self) -> DiagnosticSeverity {
        self.inner
            .read()
            .await
            .lsp
            .unknown_dep_severity
            .filter(verify_severity)
            .unwrap_or(DiagnosticSeverity::WARNING)
    }
}

// verify the config is a valid severity level
fn verify_severity(d: &DiagnosticSeverity) -> bool {
    *d >= DiagnosticSeverity::ERROR && *d <= DiagnosticSeverity::HINT
}

#[derive(Default, Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LspSettings {
    #[serde(default)]
    pub use_api: Option<bool>,
    #[serde(default)]
    pub needs_update_severity: Option<DiagnosticSeverity>,
    #[serde(default)]
    pub up_to_date_severity: Option<DiagnosticSeverity>,
    #[serde(default)]
    pub unknown_dep_severity: Option<DiagnosticSeverity>,
}

#[derive(Default, Debug, Clone, Deserialize)]
pub struct InnerSettings {
    lsp: LspSettings,
}
