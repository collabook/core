#[macro_use]
extern crate serde_derive;

mod book;
mod error;
mod macros;
mod vcs;

use actix_web::middleware::{cors::Cors, Logger};
use actix_web::{http, server, App};
use crate::book::*;
use crate::vcs::*;

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
                .resource("/newbook", |r| r.method(http::Method::POST).with(new_book))
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
                    r.method(http::Method::POST).with(git_commit)
                })
                .resource("/gitlog", |r| r.method(http::Method::POST).with(git_log))
                .resource("/gitcheckout", |r| {
                    r.method(http::Method::POST).with(git_checkout)
                })
                //.resource("/gitgetremotes", |r| r.method(http::Method::POST).with(git_get_remotes))
                .resource("/gitremoteadd", |r| {
                    r.method(http::Method::POST).with(git_remote_add)
                })
                .resource("/gitpush", |r| r.method(http::Method::POST).with(git_push))
                .resource("/gitpull", |r| r.method(http::Method::POST).with(git_pull))
                .resource("/gitswitchbranch", |r| r.method(http::Method::POST).with(git_switch_branch))
                .resource("/gitcreatebranch", |r| r.method(http::Method::POST).with(git_create_branch))
                .resource("/gitrebasecontinue", |r| r.method(http::Method::POST).with(git_rebase_continue))
                .register()
        })
    })
    .bind("localhost:8088")
    .unwrap()
    .run();
}
