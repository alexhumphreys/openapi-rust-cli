use clap::ArgMatches;
use clap::{App, Arg, SubCommand};
use openapiv3::OpenAPI;
use openapiv3::PathItem;
use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;
use std::error::Error;
use std::fs;

#[derive(Debug)]
struct Endpoint {
    name: String,
    method: String,
    path: String,
    params: Vec<Parameter>,
}

#[derive(Debug)]
struct Parameter {
    name: String,
    location: ParameterLocation,
    required: bool,
    param_type: String,
}

#[derive(Debug)]
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

        let mut params = Vec::new();

        println!("item {:?}", &path_item);
        println!("parameters {:?}", &path_item.parameters);
        println!("get {:?}", &path_item.get);
        println!("post {:?}", &path_item.post);
        println!("put {:?}", &path_item.put);
        println!("delete {:?}", &path_item.delete);
        for param in &path_item.parameters {
            println!("param {:?}", param);
            match param.as_item() {
                Some(paramx) => {
                    match paramx {
                        openapiv3::Parameter::Query {
                            parameter_data,
                            allow_reserved: _,
                            style: _,
                            allow_empty_value: _,
                        } => {
                            println!("Q param data {:?}", parameter_data);
                            println!("Q paramx {:?}", paramx);
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
                            println!("Q param data {:?}", parameter_data);
                            println!("Q paramx {:?}", paramx);
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
                    }
                }
                None => {
                    todo!()
                }
            }
        }

        // Handle request body if present
        if let Some(request_body) = &op.request_body {
            match request_body.clone().into_item() {
                Some(rb) => {
                    params.push(Parameter {
                        name: "body".to_string(),
                        location: ParameterLocation::Body,
                        required: rb.required,
                        param_type: "json".to_string(),
                    });
                }
                None => todo!(),
            }
        }

        endpoints.push(Endpoint {
            name,
            method: method.to_string(),
            path: path.to_string(),
            params,
        });
    }
}

fn build_cli(endpoints: &[Endpoint]) -> App {
    let mut app = App::new("api-client")
        .version("1.0")
        .author("Generated from OpenAPI spec");

    for endpoint in endpoints {
        let mut cmd = SubCommand::with_name(&endpoint.name);

        for param in &endpoint.params {
            let mut arg = Arg::with_name(&param.name).long(&param.name);

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

async fn execute_request<'a>(
    client: &Client,
    endpoint: &Endpoint,
    matches: &'a clap::ArgMatches<'a>,
    base_url: &str,
) -> Result<Value, Box<dyn Error>> {
    let mut url = format!("{}{}", base_url, endpoint.path);
    let mut query_params = Vec::new();
    let mut body: Option<Value> = None;

    for param in &endpoint.params {
        if let Some(value) = matches.value_of(&param.name) {
            match param.location {
                ParameterLocation::Query => {
                    query_params.push(format!("{}={}", param.name, value));
                }
                ParameterLocation::Body => {
                    body = Some(serde_json::from_str(value)?);
                }
                ParameterLocation::Path => {
                    url = url.replace(&format!("{{{}}}", param.name), value);
                }
            }
        }
    }

    if !query_params.is_empty() {
        url = format!("{}?{}", url, query_params.join("&"));
    }

    let mut request = match endpoint.method.as_str() {
        "get" => client.get(&url),
        "post" => client.post(&url),
        "put" => client.put(&url),
        "delete" => client.delete(&url),
        _ => return Err("Unsupported method".into()),
    };

    if let Some(body_value) = body {
        request = request.json(&body_value);
    }

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
    let app = build_cli(&endpoints);
    let matches = app.get_matches();

    // Get base URL from spec
    let base_url = match spec.servers.first() {
        Some(server) => server.url.clone(),
        _ => "http://localhost:3000".to_string(),
    };

    // Create HTTP client
    let client = Client::new();

    // Execute the matching command
    for endpoint in &endpoints {
        if let Some(cmd_matches) = matches.subcommand_matches(&endpoint.name) {
            let result = execute_request(&client, endpoint, cmd_matches, &base_url).await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            return Ok(());
        }
    }

    Ok(())
}
