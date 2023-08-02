use hyper::client::HttpConnector;
use hyper::{Body, Request};
use hyper_rustls::HttpsConnector;
use semver::Version;
use serde::Deserialize;

pub type HyperClient = hyper::Client<HttpsConnector<HttpConnector>>;

#[derive(Deserialize)]
struct CrateInner {
    pub max_stable_version: Version,
}

#[derive(Deserialize)]
struct Crate {
    #[serde(rename = "crate")]
    pub inner: CrateInner,
}

#[derive(Debug)]
enum CrateError {
    Http(hyper::http::Error),
    Hyper(hyper::Error),
}

async fn get_latest_version(client: HyperClient, crate_name: &str) -> Result<Version, CrateError> {
    let response = client
        .request(
            Request::builder()
                .uri(&format!("https://crates.io/api/v1/crates/{crate_name}"))
                .header(
                    "User-Agent",
                    "crates-lsp (github.com/MathiasPius/crates-lsp)",
                )
                .header("Accept", "application/json")
                .body(Body::empty())
                .map_err(CrateError::Http)?,
        )
        .await
        .map_err(CrateError::Hyper)?;

    let body = hyper::body::to_bytes(response.into_body())
        .await
        .map_err(CrateError::Hyper)?;

    let stringified = String::from_utf8_lossy(&body);

    let details: Crate = serde_json::from_str(&stringified).unwrap();

    Ok(details.inner.max_stable_version)
}

#[cfg(test)]
mod tests {
    use crate::check::get_latest_version;

    #[tokio::test]
    async fn test_get_latest() {
        let https = hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()
            .https_only()
            .enable_http1()
            .build();
        let client = hyper::Client::builder().build(https);

        let result = get_latest_version(client, "serde_json").await;

        println!("{result:#?}");
    }
}
