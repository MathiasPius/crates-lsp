use std::sync::Arc;

use serde::Deserialize;
use tokio::sync::RwLock;

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
}

#[derive(Default, Debug, Clone, Deserialize)]
pub struct LspSettings {
    #[serde(rename = "useApi", default)]
    pub use_api: Option<bool>,
}

#[derive(Default, Debug, Clone, Deserialize)]
pub struct InnerSettings {
    lsp: LspSettings,
}
