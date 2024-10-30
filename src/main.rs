use crate::errors::Errors;
use clap::{Arg, Command};
use percent_encoding::percent_decode_str;
use reqwest::header::HeaderMap;
use reqwest::Client;
use serde_json::Value;
use tracing::{error, info, warn, Level};
use tracing_subscriber::EnvFilter;
use url::Url;

mod errors;
mod http;
mod openapi;

fn setup_logging() {
    let subscriber = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
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
    let mut command = clap::command!().arg(
        Arg::new("config")
            .short('c')
            .long("config")
            .required(true)
            .help("Path to command configuration file"),
    );
    // Register `complete` subcommand
    command = clap_autocomplete::add_subcommand(command);

    endpoints.sort_by_key(|e| e.name.clone());
    for endpoint in endpoints {
        let mut cmd = Command::new(endpoint.name);

        for param in endpoint.params {
            let mut arg = Arg::new(param.name.to_owned()); // Use string slice directly

            arg = arg.required(param.required);

            arg = match param.location {
                openapi::ParameterLocation::Query => arg.long(param.name),
                openapi::ParameterLocation::Body => arg.help("JSON string for request body"),
                openapi::ParameterLocation::Path => arg,
                openapi::ParameterLocation::Header => arg.long(param.name),
            };

            cmd = cmd.arg(arg)
        }

        command = command.subcommand(cmd);
    }

    command
}

fn get_config_path() -> Option<String> {
    // First parse: Just get the config file path
    let initial_cmd = Command::new("myapp")
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .required(true)
                .help("Path to openapi configuration file"),
        )
        .disable_help_flag(true)
        .disable_help_subcommand(true)
        .ignore_errors(true);

    match initial_cmd.try_get_matches() {
        Ok(matches) => matches.get_one::<String>("config").cloned(),
        Err(_e) => {
            error!("here for some reason");
            Some("openapi.yaml".to_string())
        }
    }
}

#[tokio::main]
async fn main() -> miette::Result<(), Errors> {
    setup_logging();

    let path = get_config_path().unwrap_or("openapi.yaml".to_string());

    // Parse OpenAPI spec
    // Extract endpoints
    let parsed_openapi = openapi::parse_endpoints(path.as_str())?;

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

        warn!("base url {}", base_url);

        // Create HTTP client
        let client = Client::new();

        // Execute the matching command
        for endpoint in parsed_openapi.endpoints {
            if let Some(cmd_matches) = matches.subcommand_matches(&endpoint.name) {
                let result =
                    http::execute_request(&client, endpoint, cmd_matches.clone(), &base_url)
                        .await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
                return Ok(());
            }
        }
    }

    Ok(())
}
