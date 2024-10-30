use miette::miette;
use percent_encoding::percent_decode_str;
use reqwest::header::HeaderMap;
use reqwest::Client;
use serde_json::Value;
use tracing::{debug, error, info, warn};
use url::Url;

use crate::{errors::Errors, openapi};

fn join_url(base: &str, path: &str) -> Result<Url, url::ParseError> {
    let base_url = Url::parse(base)?;
    let path_without_leading_slash = path.strip_prefix('/').unwrap_or(path);
    Ok(base_url.join(path_without_leading_slash)?)
}

fn handle_url_path(
    endpoint: openapi::Endpoint,
    matches: clap::ArgMatches,
    base_url: &str,
) -> miette::Result<Url, Errors> {
    let url = Url::parse(base_url)?;
    debug!("base url {}", url);

    // Get the current path from the base URL
    let base_path = url.path().to_string();
    debug!("base path {}", base_path);
    debug!("endpoint path {}", endpoint.path);

    // Combine base path with endpoint path
    let mut url = join_url(base_url, endpoint.path.as_str())?;
    debug!("pre interpolation url {}", url);

    // interpolate path parameters
    // this builds the full url, takes the path, interpolates that path, then sets the new path on
    // the url
    let mut path_for_interpolation = url.path().to_string();
    info!("Process path parameters");
    for param in endpoint.params.clone() {
        if let Some(value) = matches.get_one::<String>(param.name.as_str()) {
            if matches!(param.location, openapi::ParameterLocation::Path) {
                // First decode any percent-encoded characters in the path
                let decoded_path = percent_decode_str(&path_for_interpolation)
                    .decode_utf8_lossy()
                    .into_owned();
                // Then do the replacement
                path_for_interpolation =
                    decoded_path.replace(&format!("{{{}}}", param.name), value);
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
    url.set_path(path_for_interpolation.as_str());

    warn!("final url {}", url);
    Ok(url)
}

pub async fn execute_request(
    endpoint: openapi::Endpoint,
    matches: clap::ArgMatches,
    base_url: &str,
) -> miette::Result<Value, Errors> {
    let mut final_url = handle_url_path(endpoint.clone(), matches.clone(), base_url)?;

    // Create HTTP client
    let client = Client::new();

    // Process query and body parameters
    let mut body: Option<Value> = None;

    let mut headers = HeaderMap::new();
    for param in endpoint.params {
        if let Some(value) = matches.get_one::<String>(param.name.as_str()) {
            match param.location {
                openapi::ParameterLocation::Query => {
                    final_url.query_pairs_mut().append_pair(&param.name, value);
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

    // Build the request based on the HTTP method
    let mut request = match endpoint.method.to_lowercase().as_str() {
        "get" => client.get(final_url),
        "post" => client.post(final_url),
        "put" => client.put(final_url),
        "delete" => client.delete(final_url),
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
