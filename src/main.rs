extern crate actix_web;
#[macro_use]
extern crate serde_derive;
extern crate env_logger;
extern crate serde_json;

use actix_web::middleware::{cors::Cors, Logger};
use actix_web::{http, server, App, Json, Path, Result};
use std::fs;
use std::env;
use std::path;
use std::io::prelude::*;

#[derive(Deserialize,Debug)]
struct Files {
    subfiles: Vec<String>,
    subfolders: Vec<String>
}

#[derive(Deserialize,Debug)]
struct Folders {
    name: String,
    subfolders: Option<Vec<Box<Folders>>>
}

impl Folders {
    fn new(name: &str) -> Self {

        let data = format!(r#"{{
            "name": "{}",
            "subfolders": [
                {{
                    "name": "chapter1",
                    "subfolders": [
                        {{
                        "name": "scene1"
                        }}
                    ]
                }},
                {{
                    "name": "chapter2",
                    "subfolders": [
                        {{
                        "name": "scene1",
                        "subfolders": [
                            {{
                            "name": "section1"
                            }}
                        ]
                        }}
                    ]
                }},
                {{
                    "name": "chapter3",
                    "subfolders": [
                        {{
                        "name": "scene1"
                        }}
                    ]
                }}
            ]
        }}"#, name);

        serde_json::from_str(&data).unwrap()
    }

    fn mkdirs(&self) {
        match &self.subfolders {
            Some(subfolders) => {
                fs::create_dir_all(&self.name).unwrap();
                let path = path::Path::new(&self.name);
                env::set_current_dir(&path).unwrap();
                for subs in subfolders.iter() {
                    subs.mkdirs();
                }
                env::set_current_dir(path::Path::new("..")).unwrap();
            },
            None => {
                fs::create_dir_all(&self.name).unwrap();
            }
        }
    }
}

#[derive(Serialize)]
struct filenames {
    files: Vec<String>
}

fn new_book(info: Path<(String, String)>) -> Result<Json<filenames>> {
    let book = Folders::new(&info.1);
    book.mkdirs();
    Ok(Json(filenames {files: vec!["file1".to_string(), "file2".to_string()]}))
}

fn save_book(info: Json<Vec<Save>>) -> Result<String> {
    for file in info.iter() {
        let mut f = fs::File::create(&file.file).unwrap();
        f.write_all(&file.content.as_bytes()).unwrap();
    }
    println!("{:?}", info);
    Ok(format!("saved book"))
}

fn delete_file(info: Path<(String,)>) -> Result<String> {
    fs::remove_dir_all(&info.0).unwrap();
    Ok(format!("deleted file"))
}

#[derive(Deserialize,Debug)]
struct Save {
    content: String,
    file: String,
}

fn save(info: Json<Save>) -> Result<String> {
    let mut f = fs::File::create(&info.file).unwrap();
    f.write_all(&info.content.as_bytes()).unwrap();
    println!("{:?}", info);
    Ok(format!("save file"))
}

fn main() {
    std::env::set_var("RUST_LOG", "actix_web=info");
    env_logger::init();
    server::new(|| {
        App::new().middleware(Logger::default()).configure(|app| {
            Cors::for_app(app)
                .allowed_methods(vec!["GET", "POST", "PUT", "DELETE"])
                .allowed_origin("http://localhost:9080")
                .supports_credentials()
                .max_age(3600)
                .resource("/newbook/{genre}/{bookname}", |r| r.method(http::Method::POST).with(new_book))
                .resource("/savebook", |r| r.method(http::Method::POST).with(save_book))
                .resource("/save", |r| r.method(http::Method::POST).with(save))
                .resource("/delete", |r| r.method(http::Method::POST).with(delete_file))
                .register()
        })
    }).bind("localhost:8088")
    .unwrap()
    .run();
}
