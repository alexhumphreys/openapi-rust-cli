use clap::{Arg, Command};
use openapiv3::OpenAPI;
use openapiv3::PathItem;
use percent_encoding::percent_decode_str;
use reqwest::Client;
use serde_json::Value;
use std::error::Error;
use std::fs;
use url::Url;

#[derive(Debug, Clone)]
struct Endpoint {
    name: String,
    method: String,
    path: String,
    params: Vec<Parameter>,
}

#[derive(Debug, Clone)]
struct Parameter {
    name: String,
    location: ParameterLocation,
    required: bool,
    param_type: String,
}

#[derive(Debug, Clone)]
enum ParameterLocation {
    Query,
    Body,
    Path,
}

fn parse_spec(spec_path: &str) -> Result<OpenAPI, Box<dyn Error>> {
    let spec_content = fs::read_to_string(spec_path)?;
    let spec: OpenAPI = serde_yaml::from_str(&spec_content)?;
    Ok(spec)
}

fn extract_endpoints(spec: &OpenAPI) -> Vec<Endpoint> {
    let mut endpoints = Vec::new();

    for (path, path_item) in spec.paths.clone().into_iter() {
        match path_item.into_item() {
            Some(path_item) => {
                add_endpoint_for_method("get", &path, &path_item, &mut endpoints);
                add_endpoint_for_method("post", &path, &path_item, &mut endpoints);
                add_endpoint_for_method("put", &path, &path_item, &mut endpoints);
                add_endpoint_for_method("delete", &path, &path_item, &mut endpoints);
            }
            None => {}
        }
    }

    endpoints
}

fn parse_params(ps: &Vec<openapiv3::ReferenceOr<openapiv3::Parameter>>) -> Vec<Parameter> {
    let mut params = Vec::new();
    for param in ps {
        println!("HERE1 {:?}", param);
        match param.as_item() {
            Some(paramx) => {
                match paramx {
                    openapiv3::Parameter::Query {
                        parameter_data,
                        allow_reserved: _,
                        style: _,
                        allow_empty_value: _,
                    } => {
                        println!("2 Query param data {:?}", parameter_data);
                        params.push(Parameter {
                            name: parameter_data.name.clone(),
                            location: ParameterLocation::Query,
                            required: parameter_data.required,
                            param_type: "string".to_string(), // Simplified type handling
                        });
                    }
                    openapiv3::Parameter::Header {
                        parameter_data: _,
                        style: _,
                    } => todo!(),
                    openapiv3::Parameter::Path {
                        parameter_data,
                        style: _,
                    } => {
                        println!("3 Path param data {:?}", parameter_data);
                        params.push(Parameter {
                            name: parameter_data.name.clone(),
                            location: ParameterLocation::Path,
                            required: parameter_data.required,
                            param_type: "string".to_string(), // Simplified type handling
                        });
                    }
                    openapiv3::Parameter::Cookie {
                        parameter_data: _,
                        style: _,
                    } => todo!(),
                };
            }
            None => {
                todo!()
            }
        }
    }
    println!("op {:?}", params);
    params
}

fn add_endpoint_for_method(
    method: &str,
    path: &str,
    path_item: &PathItem,
    endpoints: &mut Vec<Endpoint>,
) {
    let operation = match method {
        "get" => path_item.get.as_ref(),
        "post" => path_item.post.as_ref(),
        "put" => path_item.put.as_ref(),
        "delete" => path_item.delete.as_ref(),
        _ => None,
    };

    if let Some(op) = operation {
        let name = op
            .operation_id
            .clone()
            .unwrap_or_else(|| format!("{}_{}", method, path.replace("/", "_")));

        println!("op {:?}", &op.parameters);

        let mut parsed_params = parse_params(&op.parameters);

        // Handle request body if present
        if let Some(request_body) = &op.request_body {
            match request_body.clone().into_item() {
                Some(rb) => {
                    parsed_params.push(Parameter {
                        name: "body".to_string(),
                        location: ParameterLocation::Body,
                        required: rb.required,
                        param_type: "json".to_string(),
                    });
                }
                None => todo!(),
            }
        }

        println!("final parsed params {:?}", parsed_params);
        endpoints.push(Endpoint {
            name,
            method: method.to_string(),
            path: path.to_string(),
            params: parsed_params,
        });
        println!("endpoints {:?}", endpoints);
    }
}

fn build_cli(endpoints: Vec<Endpoint>) -> Command {
    let mut app = Command::new("api-client")
        .version("1.0")
        .author("Generated from OpenAPI spec");

    for endpoint in endpoints {
        let mut cmd = Command::new(endpoint.name); // Use string slice directly

        for param in endpoint.params {
            let mut arg = Arg::new(param.name.to_owned()); // Use string slice directly

            if param.required {
                arg = arg.required(true);
            }

            if matches!(param.location, ParameterLocation::Body) {
                arg = arg.help("JSON string for request body");
            }

            cmd = cmd.arg(arg);
        }

        app = app.subcommand(cmd);
    }

    app
}

async fn execute_request(
    client: &Client,
    endpoint: Endpoint,
    matches: clap::ArgMatches,
    base_url: &str,
) -> Result<Value, Box<dyn Error>> {
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
            if matches!(param.location, ParameterLocation::Path) {
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
                return Err(format!("Required parameter '{}' not provided", param.name).into());
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
                ParameterLocation::Query => {
                    url.query_pairs_mut().append_pair(&param.name, value);
                }
                ParameterLocation::Body => {
                    body = Some(serde_json::from_str(value)?);
                }
                ParameterLocation::Path => {
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
        method => return Err(format!("Unsupported HTTP method: {}", method).into()),
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
async fn main() -> Result<(), Box<dyn Error>> {
    // Parse OpenAPI spec
    let spec = parse_spec("openapi.yaml")?;

    // Extract endpoints
    let endpoints = extract_endpoints(&spec);

    // Build CLI
    let app = build_cli(endpoints.clone());
    let matches = app.get_matches();

    // Get base URL from spec
    let base_url = match spec.servers.first() {
        Some(server) => server.url.clone(),
        _ => "http://localhost:3000".to_string(),
    };

    // Create HTTP client
    let client = Client::new();

    // Execute the matching command
    for endpoint in endpoints {
        if let Some(cmd_matches) = matches.subcommand_matches(&endpoint.name) {
            let result = execute_request(&client, endpoint, cmd_matches.clone(), &base_url).await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            return Ok(());
        }
    }

    Ok(())
}
