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
    fn mkdirs(&self, tree: &HashMap<u32, File>) -> Result<()> {

        env::set_current_dir(path::Path::new(&self.location)).unwrap();

        // problem may occur if file creation is tried before its parent folder
        // although this problem is solved i dont like the solution
        for  (_id, file) in tree {
            println!("{}", &file.full_path);
            if file.is_folder == true {
                fs::create_dir_all(&file.full_path)?;
            }
        }

        for (_id, file) in tree {
            if file.is_folder == false {
                fs::File::create(&file.full_path)?;
            }
        }

        Ok(())
    }

    // check genre here
    fn create_tree(&self) -> HashMap<u32, File> {
        let mut tree = HashMap::new();

        // check pathbuff might be useful
        // come up with better way to build these structs
        let book = File::new(1, &self.name, format!("{}", &self.name), 0, true, true);
        let chap1 = File::new(2, "chap1", format!("{}/chap1", &self.name), 1, true, false);
        let chap2 = File::new(3, "chap2", format!("{}/chap2", &self.name), 1, true, false);
        let chap3 = File::new(4, "chap3", format!("{}/chap3", &self.name), 1, true, true);
        let sec1 = File::new(5,  "sec1", format!("{}/chap3/sec1", &self.name), 4, true, false);

        tree.insert(book.id, book);
        tree.insert(chap1.id, chap1);
        tree.insert(chap2.id, chap2);
        tree.insert(chap3.id, chap3);
        tree.insert(sec1.id, sec1);

        tree
    }
}

#[derive(Serialize,Debug)]
#[serde(rename_all = "camelCase")]
struct File {
    id: u32,
    name: String,
    full_path: String,
    parent: u32,
    is_visible: bool,
    is_folder: bool,
    // content is missing
    // storing content in tree.json wont do
}

impl File {
    fn new(id: u32, name: &str, path: String, parent: u32, visible: bool, folder: bool) -> Self {
        File {id: id, name: name.to_owned(), full_path: path, parent: parent, is_visible: visible, is_folder: folder}
    }
}

fn new_book(info: Json<NewBook>) -> Result<String> {

    let tree = info.create_tree();
    info.mkdirs(&tree)?;
    let ser = serde_json::to_string(&tree)?;

    // find a better way to do this
    env::set_current_dir(path::Path::new(&info.location))?;
    env::set_current_dir(path::Path::new(&info.name))?;

    let cur_dir = env::current_dir()?;
    println!("{}", cur_dir.display());

    let mut file = fs::File::create("tree.json")?;
    file.write_all(&ser.as_bytes())?;
    println!("{:?}", ser);

    Ok(ser)
}

#[derive(Serialize,Deserialize,Debug)]
struct Openbook {
    location: String
}

fn open_book(info: Json<Openbook>) -> Result<String> {

    env::set_current_dir(path::Path::new(&info.location))?;
    let file = fs::File::open("tree.json");
    match file {
        Ok(mut f) => {
            let mut content = String::new();
            f.read_to_string(&mut content)?;
            println!("{}", content);
            Ok(content)
        },
        Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => {
            // proper resp status must be set
            Ok(format!("Not a book"))
        },
        Err(_) => {
            // proper resp status must be set
            Ok(format!("Unknown error"))
        },
    }

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
