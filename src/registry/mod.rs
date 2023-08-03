use semver::Version;

#[tower_lsp::async_trait]
pub trait CrateRegistry {
    async fn get_latest_version(&mut self, crate_name: &str) -> Option<Version>;
}
