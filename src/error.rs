use actix_web::HttpResponse;
use std::fmt;

#[derive(Debug)]
pub struct MyError(pub String);

impl fmt::Display for MyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for MyError {
    fn description(&self) -> &str {
        &self.0
    }

    fn cause(&self) -> Option<&dyn std::error::Error> {
        None
    }
}

impl actix_web::error::ResponseError for MyError {
    fn error_response(&self) -> HttpResponse {
        HttpResponse::BadRequest().body(&self.0)
    }
}

impl From<&str> for MyError {
    fn from(e: &str) -> MyError {
        MyError(e.to_owned())
    }
}

impl From<String> for MyError {
    fn from(e: String) -> MyError {
        MyError(e)
    }
}

impl From<std::path::StripPrefixError> for MyError {
    fn from(e: std::path::StripPrefixError) -> MyError {
        MyError(e.to_string())
    }
}

impl From<std::io::Error> for MyError {
    fn from(e: std::io::Error) -> MyError {
        MyError(e.to_string())
    }
}

impl From<serde_json::Error> for MyError {
    fn from(e: serde_json::Error) -> MyError {
        MyError(e.to_string())
    }
}

impl From<xdg::BaseDirectoriesError> for MyError {
    fn from(e: xdg::BaseDirectoriesError) -> MyError {
        MyError(e.to_string())
    }
}

impl From<toml::de::Error> for MyError {
    fn from(e: toml::de::Error) -> MyError {
        MyError(e.to_string())
    }
}

impl From<toml::ser::Error> for MyError {
    fn from(e: toml::ser::Error) -> MyError {
        MyError(e.to_string())
    }
}

impl From<git2::Error> for MyError {
    fn from(e: git2::Error) -> MyError {
        MyError(e.message().to_string())
    }
}
