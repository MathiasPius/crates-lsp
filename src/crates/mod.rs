pub mod api;
pub mod cache;
pub mod sparse;

use std::collections::HashMap;

use async_trait::async_trait;
use reqwest::{Client, Error};
use semver::Version;
use serde::Deserialize;
use time::OffsetDateTime;
use tokio::sync::mpsc;

use self::cache::{CachedVersion, CrateCache};

#[allow(dead_code)]
#[derive(Debug)]
pub enum CrateError {
    NoVersionsFound,
    InvalidCrateName(String),
    Transport(Box<dyn std::error::Error + Send>),
    Deserialization(serde_json::Error),
    Reqwest(Error),
}

impl CrateError {
    pub fn transport(error: impl std::error::Error + Send + 'static) -> Self {
        CrateError::Transport(Box::new(error))
    }
}

impl From<Error> for CrateError {
    fn from(value: Error) -> Self {
        Self::Reqwest(value)
    }
}

#[derive(Deserialize)]
pub struct Crate {
    pub name: String,
}

#[derive(Deserialize)]
struct Crates {
    pub crates: Vec<Crate>,
}

#[async_trait]
pub trait CrateLookup: Clone + Send + 'static {
    fn client(&self) -> &Client;
    async fn search_crates(&self, crate_name: &String) -> Result<Vec<Crate>, CrateError> {
        let response = self
            .client()
            .get(&format!(
                "https://crates.io/api/v1/crates?q={}&per_page=5",
                crate_name
            ))
            .send()
            .await
            .map_err(CrateError::transport)?;

        let details: Crates = response.json().await?;
        Ok(details.crates)
    }

    async fn get_latest_version(self, crate_name: String) -> Result<Version, CrateError>;

    // How long to cache a result for.
    fn time_to_live(_version: &Option<Version>) -> time::Duration {
        time::Duration::days(1)
    }

    async fn fetch_versions(
        &self,
        cache: CrateCache,
        crate_names: &[&str],
    ) -> HashMap<String, Option<Version>> {
        let crate_names: Vec<_> = crate_names.iter().map(|name| name.to_string()).collect();

        let mut versions = HashMap::new();

        let mut dispatched_tasks = 0;
        let (tx, mut rx) = mpsc::channel(crate_names.len());
        for crate_name in crate_names {
            let tx = tx.clone();

            match cache.get(&crate_name).await {
                CachedVersion::Known(version) => {
                    versions.insert(crate_name, Some(version));
                }
                CachedVersion::DoesNotExist => {
                    versions.insert(crate_name, None);
                }
                CachedVersion::Unknown => {
                    dispatched_tasks += 1;
                    let cloned_self = self.clone();

                    tokio::spawn(async move {
                        match cloned_self.get_latest_version(crate_name.clone()).await {
                            Ok(version) => tx.send((crate_name, Some(version))).await,
                            Err(err) => {
                                println!("{:?}", err);
                                tx.send((crate_name, None)).await
                            }
                        }
                    });
                }
            };
        }

        for _ in 0..dispatched_tasks {
            let Some((name, version)) = rx.recv().await else {
                // If the receiver is broken, just ignore the rest of the dispatched tasks
                // and return whatever we have already.
                break;
            };

            // Set 24h expiration regardless of whether a package was found or not.
            let expires_at = OffsetDateTime::now_utc().saturating_add(Self::time_to_live(&version));

            // Store the result in the cache.
            cache.put(&name, version.clone(), expires_at).await;

            versions.insert(name, version);
        }

        versions
    }
}

pub fn default_client() -> Client {
    _default_client().unwrap_or_default()
}
fn _default_client() -> reqwest::Result<Client> {
    let builder = Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("crates-lsp (github.com/MathiasPius/crates-lsp)");

    if let Ok(proxy) = std::env::var("https_proxy") {
        if let Ok(proxy) = reqwest::Proxy::all(proxy) {
            return builder.proxy(proxy).build();
        }
    };
    builder.build()
}
