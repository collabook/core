#[macro_use]
extern crate serde_derive;

mod book;
mod error;
mod macros;
mod vcs;

use actix_web::middleware::{cors::Cors, Logger};
use actix_web::{http, server, App};
//use std::path::Path;
use crate::book::*;
use crate::vcs::*;

//TODO: impl my own json type for better error msg on Deserialize error

// websockets might be a better idea
fn main() {
    std::env::set_var("RUST_LOG", "actix_web=info");
    env_logger::init();
    server::new(|| {
        App::new().middleware(Logger::default()).configure(|app| {
            Cors::for_app(app)
                .allowed_methods(vec!["GET", "POST", "PUT", "DELETE"])
                .send_wildcard()
                .max_age(3600)
                .resource("/author", |r| {
                    r.method(http::Method::GET).f(book::get_author);
                    r.method(http::Method::POST).with(create_author);
                })
                .resource("/newbook", |r| r.method(http::Method::POST).with(new_book::<std::path::PathBuf>))
                .resource("/openbook", |r| {
                    r.method(http::Method::POST).with(open_book)
                })
                .resource("/newfile", |r| r.method(http::Method::POST).with(new_file))
                .resource("/savefile", |r| {
                    r.method(http::Method::POST).with(save_file)
                })
                .resource("/deletefile", |r| {
                    r.method(http::Method::POST).with(delete_file)
                })
                .resource("/savesynopsis", |r| {
                    r.method(http::Method::POST).with(save_synopsis)
                })
                //.resource("/gitadd", |r| r.method(http::Method::POST).with(git_add_all))
                .resource("/gitcommit", |r| {
                    r.method(http::Method::POST).with(commit_request)
                })
                .resource("/gitlog", |r| r.method(http::Method::POST).with(log_request))
                .resource("/gitcheckout", |r| {
                    r.method(http::Method::POST).with(checkout_request)
                })
                .resource("/gitgetremotes", |r| r.method(http::Method::POST).with(get_remote_request))
                .resource("/gitremoteadd", |r| {
                    r.method(http::Method::POST).with(remote_add_request)
                })
                .resource("/gitpush", |r| r.method(http::Method::POST).with(push_request))
                .resource("/gitpull", |r| r.method(http::Method::POST).with(pull_request))
                .resource("/gitswitchbranch", |r| r.method(http::Method::POST).with(switch_branch_request))
                .resource("/gitcreatebranch", |r|
                r.method(http::Method::POST).with(create_branch_request))
                .resource("/gitrebasecontinue", |r|
                r.method(http::Method::POST).with(rebase_request))
                .resource("/gitrebasecontinue", |r| r.method(http::Method::POST).with(rebase_continue_request))
               .register()
        })
    })
    .bind("localhost:8088")
    .unwrap()
    .run();
}
