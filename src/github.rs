use crate::book::Author;
use crate::error::MyError;
//use crate::vcs;
use actix_web::{HttpResponse, Json, Responder};

pub trait HttpSend {
    fn send(&self, request: reqwest::RequestBuilder) -> Result<reqwest::Response, MyError>;
}

pub trait AccessToken {
    fn get(&self) -> &str;
}

pub struct Sender;
impl HttpSend for Sender {
    fn send(&self, request: reqwest::RequestBuilder) -> Result<reqwest::Response, MyError> {
        Ok(request.send()?)
    }
}

#[derive(Deserialize, Debug)]
pub struct GithubApiError {
    documentation_url: String,
    message: String,
}

pub struct GithubClient<A: AccessToken, S: HttpSend = Sender> {
    client: reqwest::Client,
    sender: S,
    token: A,
}

impl<A: AccessToken> GithubClient<A, Sender> {
    pub fn new(a: A) -> GithubClient<A, Sender> {
        GithubClient {
            client: reqwest::Client::new(),
            sender: Sender,
            token: a,
        }
    }
}

impl<A: AccessToken, S: HttpSend> GithubClient<A, S> {
    #[cfg(test)]
    pub fn with_sender(sender: S, a: A) -> GithubClient<A, S> {
        GithubClient {
            client: reqwest::Client::new(),
            sender: sender,
            token: a,
        }
    }

    pub fn create_repo(&self, name: &str, description: Option<&str>) -> Result<(), MyError> {
        let mut body = std::collections::HashMap::new();
        body.insert("name", name);
        if let Some(ref des) = description {
            body.insert("description", des.as_ref());
        };

        let request = self
            .client
            .post("https://api.github.com/user/repos")
            //TODO: .bearer_auth should be used here
            .header(
                reqwest::header::AUTHORIZATION,
                format!("token {}", self.token.get()),
            )
            .json(&body);

        let mut resp = self.sender.send(request)?;

        if resp.status().is_success() {
            Ok(())
        } else {
            debug!("{:?}", resp);
            let error: GithubApiError = resp.json()?;
            Err(MyError(error.message))
        }
    }

    pub fn delete_repo(&self, owner: &str, name: &str) -> Result<(), MyError> {
        let request = self
            .client
            .delete(&format!("https://api.github.com/repos/{}/{}", owner, name))
            .header(
                reqwest::header::AUTHORIZATION,
                format!("token {}", self.token.get()),
            );

        let mut resp = self.sender.send(request)?;

        if resp.status().is_success() {
            Ok(())
        } else {
            debug!("{:?}", resp);
            let error: GithubApiError = resp.json()?;
            Err(MyError(error.message))
        }
    }

    pub fn fork_repo(&self, owner: &str, repo: &str) -> Result<(), MyError> {
        let request = self
            .client
            .post(&format!(
                "https://api.github.com/repos/{}/{}/forks",
                owner, repo
            ))
            .header(
                reqwest::header::AUTHORIZATION,
                format!("token {}", self.token.get()),
            );

        let mut resp = self.sender.send(request)?;

        if resp.status().is_success() {
            Ok(())
        } else {
            let error: GithubApiError = resp.json()?;
            Err(MyError(error.message))
        }
    }

}

#[derive(Serialize, Deserialize, Debug)]
pub struct CreateRepoRequest {
    name: String,
    description: Option<String>,
}

pub fn github_create_repo_request(
    info: Json<CreateRepoRequest>,
) -> Result<impl Responder, MyError> {
    let author = Author::read_from_disk()?;
    let _ = GithubClient::new(author)
        .create_repo(&info.name, info.description.as_ref().map(String::as_ref))?;
    Ok(HttpResponse::Ok())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DeleteRepoRequest {
    name: String,
    owner: String,
}

pub fn github_delete_repo_request(
    info: Json<DeleteRepoRequest>,
) -> Result<impl Responder, MyError> {
    let author = Author::read_from_disk()?;
    let _ = GithubClient::new(author).delete_repo(&info.owner, &info.name)?;
    Ok(HttpResponse::Ok())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ForkRepoRequest {
    name: String,
    owner: String,
}

pub fn github_fork_repo_request(info: Json<ForkRepoRequest>) -> Result<impl Responder, MyError> {
    let author = Author::read_from_disk()?;
    let _ = GithubClient::new(author).fork_repo(&info.owner, &info.name)?;
    Ok(HttpResponse::Ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::response;
    use std::cell::RefCell;

    pub struct MockSender(RefCell<response::Builder>, &'static str);
    impl HttpSend for MockSender {
        fn send(&self, _: reqwest::RequestBuilder) -> Result<reqwest::Response, MyError> {
            let mut builder = self.0.borrow_mut();
            let response = builder.body(self.1).map_err(|e| e.to_string())?;
            let response = response.into();
            Ok(response)
        }
    }

    pub struct MockToken<'a>(&'a str);

    impl<'a> AccessToken for MockToken<'a> {
        fn get(&self) -> &str {
            &self.0
        }
    }

    fn client_with_response(
        status: u16,
        body: &'static str,
    ) -> GithubClient<MockToken, MockSender> {
        let token = MockToken("my_test_token");
        let mut builder = response::Builder::new();
        builder.status(status);
        let sender = MockSender(RefCell::new(builder), body);
        GithubClient::with_sender(sender, token)
    }

    #[test]
    fn create_repo_handles_error() {
        env_logger::init();
        let client = client_with_response(
            400,
            r#"{
        "message": "Some error message",
        "documentation_url": "url"
        }"#,
        );

        let resp_err = client
            .create_repo("test", Some("some description"))
            .unwrap_err();
        assert_eq!(resp_err, MyError("Some error message".to_string()))
    }

    #[test]
    fn create_repo_works() {
        let client = client_with_response(201, "");
        assert!(client.create_repo("test", Some("some description")).is_ok());
    }

    #[test]
    fn delete_repo_works() {
        let client = client_with_response(204, "");
        assert!(client.delete_repo("owner", "repo").is_ok());
    }

    #[test]
    fn delete_repo_handles_error() {
        let client = client_with_response(
            400,
            r#"{
        "message": "Some error message",
        "documentation_url": "url"
        }"#,
        );

        let resp_err = client.delete_repo("owner", "repo").unwrap_err();
        assert_eq!(resp_err, MyError("Some error message".to_string()));
    }

    #[test]
    fn fork_repo_works() {
        let client = client_with_response(201, "");
        assert!(client.fork_repo("owner", "repo").is_ok());
    }

    #[test]
    fn fork_repo_handles_error() {
        let client = client_with_response(
            400,
            r#"{
            "message": "Some error message",
            "documentation_url": "url"
            }"#);

        let resp_err = client.fork_repo("owner", "repo").unwrap_err();
        assert_eq!(resp_err, MyError("Some error message".to_string()));
    }

    #[ignore]
    #[test]
    fn create_repo_works_real() {
        let token = MockToken("token here");
        GithubClient::new(token)
            .create_repo("testapi", None)
            .unwrap();
    }

    #[ignore]
    #[test]
    fn delete_repo_works_real() {
        let token = MockToken("token here");
        GithubClient::new(token)
            .delete_repo("collabook", "testapi")
            .unwrap();
    }

    #[ignore]
    #[test]
    fn fork_works() {
        let token = MockToken("token here");
        GithubClient::new(token)
            .fork_repo("akhilkpdasan", "rs-attendance")
            .unwrap();
    }
}
