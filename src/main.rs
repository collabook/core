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
use std::collections::HashMap;

/*
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

*/

#[derive(Deserialize)]
struct NewBook {
    location: String,
    name: String,
    genre: String
}

impl NewBook {
    fn mkdirs(&self) -> Result<()> {
        //
        // TODO: check genre and build vec accordingly
        //
        let folders = vec!["chapter1", "chapter2", "chapter3"];
        let files = vec!["chapter1/section1", "chapter2/section2", "chapter3/section3"];
        env::set_current_dir(path::Path::new(&self.location)).unwrap();
        fs::create_dir_all(&self.name).unwrap();
        env::set_current_dir(path::Path::new(&self.name)).unwrap();
        for folder in folders.iter() {
            fs::create_dir_all(folder)?;
        }
        for file in files.iter() {
            fs::File::create(file)?;
        }
        Ok(())
    }

    // may be impl new should be made for Files struct
    fn create_tree(&self) -> Files {
        let mut tree = Files {book_tree: HashMap::new()};

        let book = self.create_structs(&self.name, "/booktest", true, true);
        let chap1 = self.create_structs("chap1", "/booktest/chap1", true, false);
        let chap2 = self.create_structs("chap2", "/booktest/chap2", true, false);
        let chap3 = self.create_structs("chap3", "/booktest/chap3", true, true);
        let sec1 = self.create_structs("sec1", "/booktest/chap3/sec1", true, false);

        tree.book_tree.insert(book.name.clone(), book);
        tree.book_tree.insert(chap1.name.clone(), chap1);
        tree.book_tree.insert(chap2.name.clone(), chap2);
        tree.book_tree.insert(chap3.name.clone(), chap3);
        tree.book_tree.insert(sec1.name.clone(), sec1);
        tree
    }

    // may be impl new should be made for File struct
    fn create_structs(&self, name: &str, path: &str, visible: bool, folder: bool) -> File {
        File {name: name.to_owned(), full_path: path.to_owned(), is_visible: visible, is_folder: folder}
    }
}

#[derive(Serialize,Debug)]
#[serde(rename_all = "camelCase")]
struct File {
    name: String,
    full_path: String,
    is_visible: bool,
    is_folder: bool,
}

#[derive(Serialize,Debug)]
#[serde(rename_all = "camelCase")]
struct Files {
    // holds a dict of all files of the book
    book_tree: HashMap<String, File>
}

fn new_book(info: Json<NewBook>) -> Result<String> {
    info.mkdirs()?;
    let tree = info.create_tree();
    let ser = serde_json::to_string(&tree)?;
    println!("{:?}", ser);
    Ok(ser)
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
    println!("{:?}", info);
    let mut f = fs::File::create(&info.file).unwrap();
    f.write_all(&info.content.as_bytes()).unwrap();
    Ok(format!("save file"))
}

// websockets might be a better idea
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
                .resource("/newbook", |r| r.method(http::Method::POST).with(new_book))
                .resource("/savebook", |r| r.method(http::Method::POST).with(save_book))
                .resource("/save", |r| r.method(http::Method::POST).with(save))
                .resource("/delete", |r| r.method(http::Method::POST).with(delete_file))
                .register()
        })
    }).bind("localhost:8088")
    .unwrap()
    .run();
}
