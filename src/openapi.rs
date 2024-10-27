use crate::errors::Errors;
use openapiv3::OpenAPI;
use openapiv3::PathItem;
use std::fs;

#[derive(Debug, Clone)]
pub struct Endpoint {
    pub name: String,
    pub method: String,
    pub path: String,
    pub params: Vec<Parameter>,
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub location: ParameterLocation,
    pub required: bool,
    pub param_type: String,
}

#[derive(Debug, Clone)]
pub enum ParameterLocation {
    Query,
    Body,
    Path,
    Header,
}

pub struct ParsedSpec {
    pub spec: OpenAPI,
    pub endpoints: Vec<Endpoint>,
}

fn parse_spec(spec_path: &str) -> miette::Result<OpenAPI, Errors> {
    let spec_content = fs::read_to_string(spec_path)?;
    let spec: OpenAPI = serde_yaml::from_str(&spec_content)?;
    Ok(spec)
}

pub fn parse_endpoints(spec_path: &str) -> miette::Result<ParsedSpec, Errors> {
    let spec = parse_spec(spec_path)?;
    let endpoints = extract_endpoints(&spec);
    Ok(ParsedSpec { spec, endpoints })
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
                        parameter_data,
                        style: _,
                    } => {
                        params.push(Parameter {
                            name: parameter_data.name.clone(),
                            location: ParameterLocation::Header,
                            required: parameter_data.required,
                            param_type: "string".to_string(), // Simplified type handling
                        });
                    }
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
