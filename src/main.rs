use crate::errors::Errors;
use clap::{Arg, Command};
use openapiv3::OpenAPI;
use openapiv3::PathItem;
use percent_encoding::percent_decode_str;
use reqwest::header::HeaderMap;
use reqwest::Client;
use serde_json::Value;
use std::fs;
use tracing::{debug, error, info, warn, Instrument, Level};
use tracing_subscriber::{fmt::format::FmtSpan, prelude::*, EnvFilter};
use url::Url;

mod errors;
mod openapi;

fn setup_logging() {
    let subscriber = tracing_subscriber::fmt()
        .with_file(true)
        .with_line_number(true)
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new(format!("{}", Level::WARN))),
        )
        .compact();

    subscriber.init();
}

fn build_cli(mut endpoints: Vec<openapi::Endpoint>) -> Command {
    let mut command = clap::command!();
    // Register `complete` subcommand
    command = clap_autocomplete::add_subcommand(command);

    endpoints.sort_by_key(|e| e.name.clone());
    for endpoint in endpoints {
        let mut cmd = Command::new(endpoint.name); // Use string slice directly

        for param in endpoint.params {
            let mut arg = Arg::new(param.name.to_owned()); // Use string slice directly

            arg = arg.required(param.required);

            arg = match param.location {
                openapi::ParameterLocation::Query => arg.long(param.name),
                openapi::ParameterLocation::Body => arg.help("JSON string for request body"),
                openapi::ParameterLocation::Path => arg,
                openapi::ParameterLocation::Header => arg.long(param.name),
            };

            cmd = cmd.arg(arg);
        }

        command = command.subcommand(cmd);
    }

    command
}

async fn execute_request(
    client: &Client,
    endpoint: openapi::Endpoint,
    matches: clap::ArgMatches,
    base_url: &str,
) -> miette::Result<Value, Errors> {
    // Parse the base URL first
    let mut url = Url::parse(base_url)?;

    // Get the current path from the base URL
    let base_path = url.path().to_string();

    // Combine base path with endpoint path and handle path parameters
    let mut final_path = if endpoint.path.starts_with('/') {
        endpoint.path.to_string()
    } else {
        format!("{}/{}", base_path.trim_end_matches('/'), endpoint.path)
    };

    // Process path parameters
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

    // Build the request based on the HTTP method
    let mut request = match endpoint.method.to_lowercase().as_str() {
        "get" => client.get(url),
        "post" => client.post(url),
        "put" => client.put(url),
        "delete" => client.delete(url),
        method => {
            return Err(Errors::UnsupportedHttpMethodError {
                method: method.to_string(),
            })
        }
    };

    request = request.header("foo", "bar");

    // Add the body if it exists
    if let Some(body_value) = body {
        request = request.json(&body_value);
    }

    // Send the request and parse the response
    let response = request.send().await?;
    let result = response.json().await?;

    Ok(result)
}

#[tokio::main]
async fn main() -> miette::Result<(), Errors> {
    setup_logging();

    // Parse OpenAPI spec
    // Extract endpoints
    let parsed_openapi = openapi::parse_endpoints("openapi.yaml")?;

    // Build CLI
    let app = build_cli(parsed_openapi.endpoints.clone());
    let app_copy = app.clone();
    let matches = app.get_matches();

    info!("running command");
    if let Some(result) = clap_autocomplete::test_subcommand(&matches, app_copy) {
        if let Err(err) = result {
            eprintln!("Insufficient permissions: {err}");
            std::process::exit(1);
        } else {
            std::process::exit(0);
        }
    } else {
        info!("running command");
        // Continue with the application logic

        // Get base URL from spec
        let base_url = match parsed_openapi.spec.servers.first() {
            Some(server) => server.url.clone(),
            _ => "http://localhost:3000".to_string(),
        };

        // Create HTTP client
        let client = Client::new();

        // Execute the matching command
        for endpoint in parsed_openapi.endpoints {
            if let Some(cmd_matches) = matches.subcommand_matches(&endpoint.name) {
                let result =
                    execute_request(&client, endpoint, cmd_matches.clone(), &base_url).await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
                return Ok(());
            }
        }
    }

    Ok(())
}
