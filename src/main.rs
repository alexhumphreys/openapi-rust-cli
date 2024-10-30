use std::io::Write;

use crate::errors::Errors;
use clap::{Arg, Command};
use tracing::{debug, error, info, warn, Level};
use tracing_subscriber::EnvFilter;

mod errors;
mod http;
mod openapi;

const DEFAULT_CONFIG: &str = "openapi.yaml";
const DEFAULT_SERVER: &str = "http://localhost:3000";

struct InitialConfig {
    config: String,
    server: Option<String>,
}

impl Default for InitialConfig {
    fn default() -> Self {
        InitialConfig {
            config: DEFAULT_CONFIG.to_string(),
            server: None,
        }
    }
}

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
    let mut command = clap::command!();

    command = command
        .arg_required_else_help(true)
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .required(true)
                .help("Path to openapi configuration file"),
        )
        .arg(
            Arg::new("server")
                .short('s')
                .long("server")
                .required(false)
                .help("override server from openapi config file"),
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

fn get_initial_config() -> InitialConfig {
    // First parse: Just get the config file path

    let initial_cmd = Command::new("myapp")
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .required(true)
                .default_value(DEFAULT_CONFIG)
                .help("Path to openapi configuration file"),
        )
        .arg(
            Arg::new("server")
                .short('s')
                .long("server")
                .required(false)
                .help("override server from openapi config file"),
        )
        .disable_help_flag(true)
        .disable_help_subcommand(true)
        .ignore_errors(true);

    match initial_cmd.try_get_matches() {
        Ok(matches) => {
            let config = matches.get_one::<String>("config");
            let server = matches.get_one::<String>("server");
            match (config, server) {
                (None, None) => InitialConfig::default(),
                (None, Some(server)) => InitialConfig {
                    config: DEFAULT_CONFIG.to_string(),
                    server: Some(server.clone()),
                },
                (Some(config), None) => InitialConfig {
                    config: config.clone(),
                    server: None,
                },
                (Some(config), Some(server)) => InitialConfig {
                    config: config.clone(),
                    server: Some(server.clone()),
                },
            }
        }
        Err(_) => InitialConfig::default(),
    }
}

#[tokio::main]
async fn main() -> miette::Result<(), Errors> {
    setup_logging();

    let initial_config = get_initial_config();

    // Parse OpenAPI spec
    // Extract endpoints
    let parsed_openapi = openapi::parse_endpoints(initial_config.config.as_str())?;

    // Build CLI
    // TODO lots of bad clones here
    let app = build_cli(parsed_openapi.endpoints.clone());
    let app_copy = app.clone();
    let mut app_copy_copy = app.clone();
    let matches = app.get_matches();

    info!("running command");
    if let Some(result) = clap_autocomplete::test_subcommand(&matches, app_copy) {
        if let Err(err) = result {
            error!("Insufficient permissions: {err}");
            std::process::exit(1);
        } else {
            std::process::exit(0);
        }
    } else {
        debug!("running command");
        // Continue with the application logic

        // Choose correct base url
        let first_server = parsed_openapi.spec.servers.first();
        let base_url = match (first_server, initial_config.server) {
            // when both, prefer server passed on command line
            (Some(_), Some(server)) => {
                debug!("choosing server from cli {}", server);
                server
            }
            (Some(server), None) => {
                debug!("choosing server from config {}", server.url.clone());
                server.url.clone()
            }
            (None, Some(server)) => server,
            // when neither, choose default
            (None, None) => DEFAULT_SERVER.to_string(),
        };

        warn!("base url {}", base_url);

        let mut ran_command = false;
        // Execute the matching command
        for endpoint in parsed_openapi.endpoints {
            if let Some(cmd_matches) = matches.subcommand_matches(&endpoint.name) {
                ran_command = true;
                let result =
                    http::execute_request(endpoint, cmd_matches.clone(), &base_url).await?;
                writeln!(
                    std::io::stdout(),
                    "{}",
                    serde_json::to_string_pretty(&result)?
                )?;
                return Ok(());
            }
        }

        if ran_command == false {
            app_copy_copy.print_help().unwrap();
        }
    }

    Ok(())
}
