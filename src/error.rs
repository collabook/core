use actix_web::HttpResponse;
use std::fmt;

#[derive(Debug)]
pub struct MyError(pub String);

impl fmt::Display for MyError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for MyError {
    fn description(&self) -> &str {
        &self.0
    }

    fn cause(&self) -> Option<&std::error::Error> {
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
