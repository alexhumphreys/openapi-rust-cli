use crate::errors::Errors;
use clap::{Arg, Command};
use openapiv3::OpenAPI;
use openapiv3::PathItem;
use percent_encoding::percent_decode_str;
use reqwest::Client;
use serde_json::Value;
use std::fs;
use url::Url;

mod errors;
mod openapi;

fn build_cli(endpoints: Vec<openapi::Endpoint>) -> Command {
    let mut app = Command::new("api-client")
        .version("1.0")
        .author("Generated from OpenAPI spec");

    for endpoint in endpoints {
        let mut cmd = Command::new(endpoint.name); // Use string slice directly

        for param in endpoint.params {
            let mut arg = Arg::new(param.name.to_owned()); // Use string slice directly

            arg = arg.required(param.required);

            arg = match param.location {
                openapi::ParameterLocation::Query => arg.long(param.name),
                openapi::ParameterLocation::Body => arg.help("JSON string for request body"),
                openapi::ParameterLocation::Path => arg,
            };

            cmd = cmd.arg(arg);
        }

        app = app.subcommand(cmd);
    }

    app
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

    println!("matches {:?}", matches);
    // Process path parameters
    for param in endpoint.params.clone() {
        println!("Looking for param: {} in matches", param.name);
        if let Some(value) = matches.get_one::<String>(param.name.as_str()) {
            println!("Found value: {}", value);
            if matches!(param.location, openapi::ParameterLocation::Path) {
                // First decode any percent-encoded characters in the path
                let decoded_path = percent_decode_str(&final_path)
                    .decode_utf8_lossy()
                    .into_owned();
                // Then do the replacement
                final_path = decoded_path.replace(&format!("{{{}}}", param.name), value);
            }
        } else {
            println!("No value found for: {}", param.name);
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
    println!("final path {:?}", &final_path);

    // Process query and body parameters
    let mut body: Option<Value> = None;

    for param in endpoint.params {
        if let Some(value) = matches.get_one::<String>(param.name.as_str()) {
            match param.location {
                openapi::ParameterLocation::Query => {
                    url.query_pairs_mut().append_pair(&param.name, value);
                }
                openapi::ParameterLocation::Body => {
                    body = Some(serde_json::from_str(value)?);
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
    //let spec = parse_spec("openapi.yaml")?;

    //let endpoints = extract_endpoints(&spec);

    // Parse OpenAPI spec
    // Extract endpoints
    let parsed_openapi = openapi::parse_endpoints("openapi.yaml")?;

    // Build CLI
    let app = build_cli(parsed_openapi.endpoints.clone());
    let matches = app.get_matches();

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
            let result = execute_request(&client, endpoint, cmd_matches.clone(), &base_url).await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            return Ok(());
        }
    }

    Ok(())
}
