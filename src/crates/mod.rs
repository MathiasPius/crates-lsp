mod cache;

use async_trait::async_trait;

#[async_trait]
pub trait CrateLookup {
    async fn get_latest_version(&self, crate_name: String);
}
