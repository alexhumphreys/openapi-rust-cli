use miette::Diagnostic;
use thiserror::Error;

#[derive(Error, Diagnostic, Debug)]
pub enum Errors {
    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    SerdeYamlError(#[from] serde_yaml::Error),

    #[error(transparent)]
    SerdeJsonError(#[from] serde_json::Error),

    #[error(transparent)]
    ReqwestError(#[from] reqwest::Error),

    #[error("Invalid header name: {name}")]
    #[diagnostic(
        code(request::invalid_header),
        help(
            "Header names must contain only visible ASCII characters (32-127) excluding separators"
        )
    )]
    InvalidHeaderName {
        name: String,
        #[source]
        source: reqwest::header::InvalidHeaderName,
    },

    #[error("Invalid header value: {name}")]
    #[diagnostic(code(request::invalid_header), help("Header names must valid strings"))]
    InvalidHeaderValue {
        name: String,
        #[source]
        source: reqwest::header::InvalidHeaderValue,
    },

    #[error(transparent)]
    UrlParseError(#[from] url::ParseError),

    #[error("Missing required parameter: '{}'", name)]
    MissingRequiredParameterError { name: String },

    #[error("Unsupport http method: '{}'", method)]
    UnsupportedHttpMethodError { method: String },
}

impl From<reqwest::header::InvalidHeaderName> for Errors {
    fn from(err: reqwest::header::InvalidHeaderName) -> Self {
        Errors::InvalidHeaderName {
            name: err.to_string(),
            source: err,
        }
    }
}

impl From<reqwest::header::InvalidHeaderValue> for Errors {
    fn from(err: reqwest::header::InvalidHeaderValue) -> Self {
        Errors::InvalidHeaderValue {
            name: err.to_string(),
            source: err,
        }
    }
}
