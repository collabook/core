extern crate actix_web;
#[macro_use]
extern crate serde_derive;
extern crate chrono;
extern crate env_logger;
extern crate git2;
extern crate serde_json;
extern crate sha1;
extern crate tempdir;
extern crate toml;
extern crate walkdir;
extern crate xdg;

mod book;
mod error;
mod macros;
//mod vcs;

use actix_web::middleware::{cors::Cors, Logger};
use actix_web::{http, server, App};
use book::*;
//use vcs::*;

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
                //.resource("/author", |r| {
                //    r.method(http::Method::GET).f(book::get_author);
                //    r.method(http::Method::POST).with(create_author);
                //})
                .resource("/newbook", |r| r.method(http::Method::POST).with(new_book))
                .resource("/openbook", |r| {
                    r.method(http::Method::POST).with(open_book)
                })
                //.resource("/savebook", |r| {
                //    r.method(http::Method::POST).with(save_book)
                //})
                //.resource("/save", |r| r.method(http::Method::POST).with(save))
                //.resource("/delete", |r| {
                //    r.method(http::Method::POST).with(delete_file)
                //})
                //.resource("/gitinit", |r| r.method(http::Method::POST).with(git_init))
                //.resource("/gitadd", |r| r.method(http::Method::POST).with(git_add))
                //.resource("/gitcommit", |r| {
                //    r.method(http::Method::POST).with(git_commit)
                //})
                //.resource("/gitlog", |r| r.method(http::Method::POST).with(git_log))
                //.resource("/gitcheckout", |r| {
                //    r.method(http::Method::POST).with(git_checkout)
                //})
                //.resource("/gitremoteadd", |r| {
                //    r.method(http::Method::POST).with(git_remote_add)
                //})
                //.resource("/gitpush", |r| r.method(http::Method::POST).with(git_push))
                .register()
        })
    })
    .bind("localhost:8088")
    .unwrap()
    .run();
}
