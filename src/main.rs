extern crate actix_web;
#[macro_use]
extern crate serde_derive;
extern crate env_logger;
extern crate serde_json;

use actix_web::middleware::{cors::Cors, Logger};
use actix_web::{http, server, App, Json, Path, Result};
use std::fs;
use std::path;
use std::io::prelude::*;
use std::collections::HashMap;

#[allow(unused)]
#[derive(Deserialize)]
struct NewBook {
    location: String,
    name: String,
    genre: String,
}

fn mkdirs(location: &str, tree: &HashMap<u32, File>) -> Result<()> {

    for  (_id, file) in tree {
        if file.is_folder == true {
            fs::create_dir_all(format!("{}/{}", location, file.full_path))?;
        }
    }

    for (_id, file) in tree {
        if file.is_folder == false {
            fs::File::create(format!("{}/{}", location, file.full_path))?;
        }
    }

    Ok(())
}

impl NewBook {
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

#[derive(Serialize,Deserialize,Debug)]
#[serde(rename_all = "camelCase")]
struct File {
    id: u32,
    name: String,
    full_path: String,
    parent: u32,
    is_visible: bool,
    is_folder: bool,
}

impl File {
    fn new(id: u32, name: &str, path: String, parent: u32, visible: bool, folder: bool) -> Self {
        File {id: id, name: name.to_owned(), full_path: path, parent: parent, is_visible: visible, is_folder: folder}
    }
}

// should also have a title
#[derive(Serialize,Deserialize,Debug)]
struct Synopsis {
    id: u32,
    content: String
}

impl Synopsis {
    fn new(tree: &HashMap<u32,File>) -> Result<Vec<Self>> {
        let mut vec_synopsis = Vec::new();
        for id in tree.keys() {
            vec_synopsis.push(Synopsis {id: id.clone(), content: format!("")});
        }
        Ok(vec_synopsis)
    }
}

// should also create an empty hashmap for content
fn new_book(info: Json<NewBook>) -> Result<String> {

    let tree = info.create_tree();
    // this moved function is not yet tested
    mkdirs(&info.location, &tree)?;
    let ser_tree = serde_json::to_string(&tree)?;

    let mut path = path::PathBuf::from(&info.location);
    path.push(&info.name);
    path.push("tree.json");

    let mut file = fs::File::create(&path)?;
    file.write_all(&ser_tree.as_bytes())?;

    path.pop(); // remove tree.json
    // TODO: should be called ::from_tree
    let synopsis = Synopsis::new(&tree)?;
    let ser_synopsis = serde_json::to_string(&synopsis)?;
    path.push("synopsis.json");
    let mut file = fs::File::create(&path)?;
    file.write_all(ser_synopsis.as_bytes())?;

    // create contents tree
    path.pop(); //remove synopsis.json
    path.pop(); // book rootdir is part of full path inside the File struct
    let content = read_content(&tree, &path.to_str().unwrap());

    let openbook_response = OpenBookResponse {tree: tree, content: content, synopsis: synopsis};
    Ok(serde_json::to_string(&openbook_response)?)
}

// create a hashmap of content and fileid
fn read_content(tree: &HashMap<u32, File>, loc: &str) -> HashMap<u32, String> {
    let mut content = HashMap::new();
    for (id, file) in tree {
        if file.is_folder == false  {
            let mut buf = String::new();

            let mut path = path::PathBuf::from(loc);
            path.push(&file.full_path);
            let mut f = fs::File::open(&path).unwrap();
            f.read_to_string(&mut buf).unwrap();
            content.insert(id.clone(), buf);
        }
    }
    content
}

// might have to rename this as it is used in savebook as well
#[derive(Serialize,Debug)]
struct OpenBookResponse {
    tree: HashMap<u32, File>,
    content: HashMap<u32, String>,
    synopsis: Vec<Synopsis>
}



#[derive(Serialize,Deserialize,Debug)]
struct Openbook {
    location: String
}

#[derive(Serialize,Deserialize,Debug)]
struct SaveSynopsis {
    location: String,
    synopsis: Vec<Synopsis>
}

fn save_synopsis(info: Json<SaveSynopsis>) -> Result<String> {
    println!("{:?}", info.location);
    let mut path = path::PathBuf::from(&info.location);

    path.push("synopsis.json");
    let  mut f = fs::File::create(&path)?;
    let ser_synopsis = serde_json::to_string(&info.synopsis)?;
    f.write(ser_synopsis.as_bytes())?;
    Ok(format!("synopsis changed"))
}

fn open_book(info: Json<Openbook>) -> Result<String> {

    let mut path = path::PathBuf::from(&info.location);
    path.push("tree.json");
    let file = fs::File::open(&path);
    match file {
        Ok(f) => {
            let tree: HashMap<u32, File> = serde_json::from_reader(f)?;

            path.pop(); // pop tree.json

            path.push("synopsis.json");
            let synopsis_file = fs::File::open(&path)?;
            let synopsis = serde_json::from_reader(synopsis_file)?;

            path.pop(); // pop synopsis.json
            path.pop(); // pop book root dir as it is already included in fullpath of File struct
            let content = read_content(&tree, path.to_str().unwrap());
            let openbook_response = OpenBookResponse {tree, content, synopsis};

            Ok(serde_json::to_string(&openbook_response)?)
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

#[derive(Serialize,Deserialize,Debug)]
struct SaveBook {
    location: String,
    tree: HashMap<u32, File>,
    content: HashMap<u32, String>
}

fn save_book(info: Json<SaveBook>) -> Result<String> {
    // combining mkdirs and write_content might be a good idea
    let mut path = path::PathBuf::from(&info.location);
    path.pop(); // remove filename
    mkdirs(path.to_str().unwrap(), &info.tree)?;
    write_content(&path, &info.tree, &info.content)?;

    Ok(format!("saved book"))
}

fn write_content(loc: &path::Path, tree: &HashMap<u32, File>, content: &HashMap<u32, String>) -> Result<()> {
    for (id, file) in tree.iter() {
        match content.get(id) {
            Some(current_content) => {
                // loc is path push pop will work
                let location = format!("{}/{}", loc.display(), file.full_path);
                let mut f = fs::File::create(location)?;
                f.write_all(current_content.as_bytes())?;
            },
            None => {
                //return error or do nothing not sure
            }
        }
    }
    Ok(())
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
                .resource("/savesynopsis", |r| r.method(http::Method::POST).with(save_synopsis))
                .resource("/save", |r| r.method(http::Method::POST).with(save))
                .resource("/delete", |r| r.method(http::Method::POST).with(delete_file))
                .register()
        })
    }).bind("localhost:8088")
    .unwrap()
    .run();
}
