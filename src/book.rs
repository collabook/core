use actix_web::{HttpResponse, Json, Responder};
use error::MyError;
use sha1::Sha1;
use std::collections::HashMap;
use std::fs;
use std::io::prelude::*;
use std::path::PathBuf;
use walkdir::WalkDir;

//use std::path::Path;
//use xdg::BaseDirectories;
//use std::ffi::OsString;
//use badrequest;
//use none;

#[derive(Serialize, Debug)]
struct Book {
    files: HashMap<String, File>,
    location: PathBuf,
    name: String,
}

#[derive(Deserialize)]
enum Genre {
    Fantasy,
    Fiction,
    Academic,
}

#[allow(unused)]
#[derive(Deserialize)]
pub struct NewBookRequest {
    location: PathBuf,
    name: String,
    genre: Genre,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct File {
    id: String,
    name: String,
    rel_path: PathBuf,
    parent: String,
    is_visible: bool,
    is_folder: bool,
    is_research: bool,
    content: Option<String>,
    synopsis: String,
}

impl File {
    fn new(name: &str, rel_path: &str, parent: String, is_folder: bool, is_research: bool) -> Self {
        let id = sha1::Sha1::from(rel_path).digest().to_string();
        let content: Option<String>;

        if is_folder {
            content = None;
        } else {
            content = Some("".to_string());
        }

        File {
            id,
            name: name.to_owned(),
            rel_path: PathBuf::from(rel_path),
            parent: parent.to_owned(),
            is_visible: true,
            is_folder,
            is_research,
            content,
            synopsis: "".to_owned(),
        }
    }
}

fn is_hidden(entry: &walkdir::DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| s.starts_with("."))
        .unwrap_or(false)
}

impl Book {
    fn new(new_book_req: &NewBookRequest) -> Self {
        let mut files = HashMap::new();
        let root = File::new(&new_book_req.name, "", format!("0"), true, false);
        let book = File::new("Book", "Book", root.id.clone(), true, false);
        let chap1 = File::new("chap1", "Book/chap1", book.id.clone(), true, false);
        let sec1 = File::new("sec1", "Book/chap1/sec1", chap1.id.clone(), false, false);
        files.insert(root.id.clone(), root);
        files.insert(book.id.clone(), book);
        files.insert(chap1.id.clone(), chap1);
        files.insert(sec1.id.clone(), sec1);

        Book {
            files,
            location: new_book_req.location.clone(),
            name: new_book_req.name.to_string(),
        }
    }

    fn mkdirs(&self) -> Result<(), MyError> {
        fs::create_dir_all(&self.location.join(".collabook/synopsis/"))?;
        for file in self.files.values() {
            let path = &self.location.join(&file.rel_path);
            if file.is_folder {
                fs::create_dir_all(&path)?;
            } else {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(&parent)?;
                }
                fs::File::create(path)?;
            }

            let synopsis_path = &self.location.join(".collabook/synopsis/").join(&file.id);
            fs::File::create(synopsis_path)?;
        }
        Ok(())
    }

    fn open(location: &PathBuf) -> Result<Self, MyError> {
        let mut files: HashMap<String, File> = HashMap::new();

        //check if is a collabook directory
        //
        if !&location.join(".collabook").exists() {
            Err("Not a Collabook directory".to_string())?
        }

        let book_name = location
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("Filename contains invalid utf-8")?;

        //read files from disk
        for entry in WalkDir::new(&location)
            .into_iter()
            .filter_entry(|e| !is_hidden(e))
            .filter_map(|e| e.ok())
        {
            let name = entry
                .file_name()
                .to_str()
                .ok_or("Filename contains invalid utf-8")?;

            let rel_path = entry.path().strip_prefix(&location)?;

            let parent_id = match rel_path.parent() {
                Some(parent) => parent
                    .to_str()
                    .ok_or("Filename contains invalid utf-8")
                    .map(|par| Sha1::from(par).digest().to_string()),
                None => Ok("0".to_string()),
            }?;

            let rel_path_str = rel_path.to_str().ok_or("Filename contains invalid utf-8")?;
            let is_folder = entry.file_type().is_dir();
            let is_research = rel_path.to_string_lossy().contains("Research");

            //read contents
            let mut content: Option<String>;
            if entry.file_type().is_file() {
                let mut data = String::new();
                let mut file = fs::File::open(entry.path())?;
                file.read_to_string(&mut data)?;
                content = Some(data)
            } else {
                content = None
            }

            //read synopsis
            let id = Sha1::from(rel_path_str).digest().to_string();
            let mut syn_file = fs::File::open(&location.join(".collabook/synopsis").join(&id))?;
            let mut synopsis = String::new();
            syn_file.read_to_string(&mut synopsis)?;

            let f = File {
                id,
                name: name.to_owned(),
                rel_path: PathBuf::from(rel_path.clone()),
                parent: parent_id,
                is_visible: true,
                is_folder,
                is_research,
                content,
                synopsis,
            };
            files.insert(f.id.clone(), f);
        }
        Ok(Book {
            files,
            location: location.clone(),
            name: book_name.to_string(),
        })
    }
}

pub fn new_book(info: Json<NewBookRequest>) -> Result<impl Responder, MyError> {
    let book = Book::new(&info);
    book.mkdirs()?;
    let ser_book = serde_json::to_string(&book)?;
    Ok(HttpResponse::Ok().body(ser_book))
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BookLocation {
    pub location: PathBuf,
}

pub fn open_book(info: Json<BookLocation>) -> Result<impl Responder, MyError> {
    let book = Book::open(&info.location)?;
    let ser_book = serde_json::to_string(&book)?;
    Ok(ser_book)
}

/*
#[derive(Serialize, Deserialize, Debug)]
pub struct SaveBook {
    //location should include bookname
    location: PathBuf,
    tree: Files,
    content: Option<Content>,
    synopsis: Option<Vec<Synopsis>>,
}

pub fn save_book(book: Json<SaveBook>) -> impl Responder {
    let mut path = path::PathBuf::from(&book.location);
    let path2 = path.clone();
    let file_name = none!(path2.file_name().clone(), "Filename not present");
    let book_name: String = none!(file_name.to_str(), "Filename contains invalid utf-8").to_owned();
    path.pop();

    badrequest!(mkdirs(&book.tree, &path));

    badrequest!(book.tree.to_disk(&book.location));

    match &book.content {
        Some(content) => {
            badrequest!(content.to_disk(&book.files, &book.location));
        }
        None => {}
    };

    match &book.synopsis {
        Some(synopsis) => {
            badrequest!(Synopsis::to_disk(&synopsis, &book.location));
        }
        None => {}
    };

    HttpResponse::Ok().finish()
}

pub fn delete_file(info: Path<(String,)>) -> Result<String> {
    // TODO: will have to delete from tree and synopsis
    // unimplemented
    fs::remove_dir_all(&info.0).unwrap();
    Ok("deleted file".to_string())
}

#[derive(Deserialize, Debug)]
pub struct Save {
    content: String,
    file: String,
}

pub fn save(info: Json<Save>) -> Result<String> {
    //TODO: not used
    let mut f = fs::File::create(&info.file).unwrap();
    f.write_all(&info.content.as_bytes()).unwrap();
    Ok("save file".to_string())
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type", content="args")]
pub enum AuthType {
    Plain {user: String, pass: String},
    SSHAgent,
    SSHPath {path: String}
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Author {
    pub name: String,
    pub email: String,
    pub auth: AuthType,
}

pub fn get_author(_req: &HttpRequest) -> impl Responder {
    let xdg_dirs = badrequest!(BaseDirectories::with_prefix("collabook"));
    let config = none!(xdg_dirs.find_config_file("Config.toml"), "Could not find config file");
    let mut file = badrequest!(fs::File::open(config));
    let mut contents = String::new();
    badrequest!(file.read_to_string(&mut contents));
    let author: Author = badrequest!(toml::from_str(&contents));
    HttpResponse::Ok().json(author)
}

pub fn create_author(info: Json<Author>) -> impl Responder {
    let xdg_dirs = badrequest!(BaseDirectories::with_prefix("collabook"));
    let path = badrequest!(xdg_dirs.place_config_file("Config.toml"));
    let author = info.into_inner();
    let contents = badrequest!(toml::to_string(&author));
    let mut file = badrequest!(fs::File::create(path));
    badrequest!(file.write_all(contents.as_bytes()));
    HttpResponse::Ok().finish()
}
*/
