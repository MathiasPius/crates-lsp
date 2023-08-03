use std::collections::HashMap;

use semver::Version;

pub mod api;

#[tower_lsp::async_trait]
pub trait CrateRegistry {
    async fn fetch_versions(&self, crate_names: &[&str]) -> HashMap<String, Option<Version>>;
}
