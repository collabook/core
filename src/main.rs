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

#[derive(Deserialize)]
struct NewBook {
    location: String,
    name: String,
    genre: String,
}

impl NewBook {
    fn mkdirs(&self) -> Result<()> {
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

    // check genre here
    fn create_tree(&self) -> HashMap<String, File> {
        let mut tree = HashMap::new();

        let book = File::new(&self.name, format!("/{}", &self.name), true, true);
        let chap1 = File::new("chap1", format!("/{}/chap1", &self.name), true, false);
        let chap2 = File::new("chap2", format!("/{}/chap2", &self.name), true, false);
        let chap3 = File::new("chap3", format!("/{}/chap3", &self.name), true, true);
        let sec1 = File::new("sec1",  format!("/{}/chap3/sec1", &self.name), true, false);

        tree.insert(book.name.clone(), book);
        tree.insert(chap1.name.clone(), chap1);
        tree.insert(chap2.name.clone(), chap2);
        tree.insert(chap3.name.clone(), chap3);
        tree.insert(sec1.name.clone(), sec1);

        tree
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

impl File {
    fn new(name: &str, path: String, visible: bool, folder: bool) -> Self {
        File {name: name.to_owned(), full_path: path, is_visible: visible, is_folder: folder}
    }
}

fn new_book(info: Json<NewBook>) -> Result<String> {
    info.mkdirs()?;
    let tree = info.create_tree();
    let ser = serde_json::to_string(&tree)?;

    // find a better way to do this
    env::set_current_dir(path::Path::new(&info.location))?;
    env::set_current_dir(path::Path::new(&info.name))?;

    let cur_dir = env::current_dir()?;
    println!("{}", cur_dir.display());

    let mut file = fs::File::create("foo.json")?;
    file.write_all(&ser.as_bytes())?;
    println!("{:?}", ser);

    Ok(ser)
}

#[derive(Serialize,Deserialize,Debug)]
struct Openbook {
    location: String
}

fn open_book(info: Json<Openbook>) -> Result<String> {
    // check if json file exists in folder
    // if not then return a not a book error
    // otherwise return the book tree from the file

    env::set_current_dir(path::Path::new(&info.location))?;
    let mut file = fs::File::open("foo.json")?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;
    println!("{}", content);
    Ok(content)
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
                .resource("/openbook", |r| r.method(http::Method::POST).with(open_book))
                .resource("/savebook", |r| r.method(http::Method::POST).with(save_book))
                .resource("/save", |r| r.method(http::Method::POST).with(save))
                .resource("/delete", |r| r.method(http::Method::POST).with(delete_file))
                .register()
        })
    }).bind("localhost:8088")
    .unwrap()
    .run();
}
