#[macro_use]
extern crate serde_derive;
//#[cfg_attr(test, macro_use)] extern crate serde_json;
#[macro_use]
extern crate log;

mod book;
mod bookcompiler;
mod error;
mod github;
mod macros;
mod vcs;

use crate::book::*;
use crate::bookcompiler::*;
use crate::github::*;
use crate::vcs::*;
use actix::prelude::*;
use actix_web::middleware::{cors::Cors, Logger};
use actix_web::{http, server, App};

//TODO: impl my own json type for better error msg on Deserialize error

// websockets might be a better idea
fn main() {
    std::env::set_var("RUST_LOG", "actix_web=info");
    env_logger::init();

    let _sys = actix::System::new("collabook-core");
    let addr = SyncArbiter::start(1, || BookCompiler {
        pdf_app: wkhtmltopdf::PdfApplication::new().unwrap(),
    });

    server::new(move || {
        App::with_state(AppState {
            compiler: addr.clone(),
        })
        .middleware(Logger::default())
        .configure(|app| {
            Cors::for_app(app)
                .allowed_methods(vec!["GET", "POST", "PUT", "DELETE"])
                .send_wildcard()
                .max_age(3600)
                .resource("/author", |r| {
                    r.method(http::Method::GET).f(book::get_author);
                    r.method(http::Method::POST).with(create_author);
                })
                .resource("/newbook", |r| {
                    r.method(http::Method::POST)
                        .with(new_book::<std::path::PathBuf>)
                })
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
                .resource("/gitlog", |r| {
                    r.method(http::Method::POST).with(log_request)
                })
                .resource("/gitcheckout", |r| {
                    r.method(http::Method::POST).with(checkout_request)
                })
                .resource("/gitgetremotes", |r| {
                    r.method(http::Method::POST).with(get_remote_request)
                })
                .resource("/gitremoteadd", |r| {
                    r.method(http::Method::POST).with(remote_add_request)
                })
                .resource("/gitpush", |r| {
                    r.method(http::Method::POST).with(push_request)
                })
                .resource("/gitpull", |r| {
                    r.method(http::Method::POST).with(pull_request)
                })
                .resource("/gitswitchbranch", |r| {
                    r.method(http::Method::POST).with(switch_branch_request)
                })
                .resource("/gitcreatebranch", |r| {
                    r.method(http::Method::POST).with(create_branch_request)
                })
                .resource("/gitrebase", |r| {
                    r.method(http::Method::POST).with(rebase_request)
                })
                .resource("/gitrebasecontinue", |r| {
                    r.method(http::Method::POST).with(rebase_continue_request)
                })
                .resource("/gitmerge", |r| {
                    r.method(http::Method::POST).with(merge_branch)
                })
                .resource("/gitclone", |r| {
                    r.method(http::Method::POST).with(clone_request)
                })
                .resource("/hubcreate", |r| {
                    r.method(http::Method::POST)
                        .with(github_create_repo_request)
                })
                .resource("/hubdelete", |r| {
                    r.method(http::Method::POST)
                        .with(github_delete_repo_request)
                })
                .resource("/hubfork", |r| {
                    r.method(http::Method::POST).with(github_fork_repo_request)
                })
                .resource("/syncfork", |r| {
                    r.method(http::Method::POST).with(sync_fork_request)
                })
                .resource("/compile", |r| {
                    r.method(http::Method::POST).with(compile_book)
                })
                .register()
        })
    })
    .bind("localhost:8088")
    .unwrap()
    .run();
}
