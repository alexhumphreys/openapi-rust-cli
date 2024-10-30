use percent_encoding::percent_decode_str;
use reqwest::header::HeaderMap;
use reqwest::Client;
use serde_json::Value;
use tracing::{debug, error, info, warn};
use url::Url;

use crate::{errors::Errors, openapi};

pub async fn execute_request(
    endpoint: openapi::Endpoint,
    matches: clap::ArgMatches,
    base_url: &str,
) -> miette::Result<Value, Errors> {
    // Create HTTP client
    let client = Client::new();

    let mut url = Url::parse(base_url)?;
    debug!("base url {}", url);

    // Get the current path from the base URL
    let base_path = url.path().to_string();

    // Combine base path with endpoint path and handle path parameters
    let mut final_path = if endpoint.path.starts_with('/') {
        endpoint.path.to_string()
    } else {
        format!("{}/{}", base_path.trim_end_matches('/'), endpoint.path)
    };

    info!("Process path parameters");
    for param in endpoint.params.clone() {
        if let Some(value) = matches.get_one::<String>(param.name.as_str()) {
            if matches!(param.location, openapi::ParameterLocation::Path) {
                // First decode any percent-encoded characters in the path
                let decoded_path = percent_decode_str(&final_path)
                    .decode_utf8_lossy()
                    .into_owned();
                // Then do the replacement
                final_path = decoded_path.replace(&format!("{{{}}}", param.name), value);
            }
        } else {
            // If the parameter is required, we should return an error
            if param.required {
                return Err(Errors::MissingRequiredParameterError {
                    name: param.name.clone(),
                });
            }
        }
    }

    // Set the processed path
    url.set_path(&final_path);

    // Process query and body parameters
    let mut body: Option<Value> = None;

    let mut headers = HeaderMap::new();
    for param in endpoint.params {
        if let Some(value) = matches.get_one::<String>(param.name.as_str()) {
            match param.location {
                openapi::ParameterLocation::Query => {
                    url.query_pairs_mut().append_pair(&param.name, value);
                }
                openapi::ParameterLocation::Body => {
                    body = Some(serde_json::from_str(value)?);
                }
                openapi::ParameterLocation::Header => {
                    headers.insert(
                        reqwest::header::HeaderName::from_bytes(
                            param.name.to_uppercase().as_bytes(),
                        )?,
                        reqwest::header::HeaderValue::from_str(value)?,
                    );
                }
                openapi::ParameterLocation::Path => {
                    // Already handled above
                    continue;
                }
            }
        }
    }

    if let Ok(token) = std::env::var("AUTHORIZATION_BASIC_TOKEN") {
        headers.insert(
            reqwest::header::AUTHORIZATION,
            reqwest::header::HeaderValue::from_str(format!("Basic {}", token).as_str())?,
        );
    };

    if let Ok(token) = std::env::var("AUTHORIZATION_BEARER_TOKEN") {
        headers.insert(
            reqwest::header::AUTHORIZATION,
            reqwest::header::HeaderValue::from_str(format!("Bearer {}", token).as_str())?,
        );
    };

    warn!("final url {}", url.clone());
    // Build the request based on the HTTP method
    let mut request = match endpoint.method.to_lowercase().as_str() {
        "get" => client.get(url),
        "post" => client.post(url),
        "put" => client.put(url),
        "delete" => client.delete(url),
        method => {
            error!("unsupported method {}", method.to_string());
            return Err(Errors::UnsupportedHttpMethodError {
                method: method.to_string(),
            });
        }
    };

    request = request.headers(headers);

    // Add the body if it exists
    if let Some(body_value) = body {
        request = request.json(&body_value);
    }

    // Send the request and parse the response
    let response = request.send().await?;
    let result = response.json().await?;

    Ok(result)
}
