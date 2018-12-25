use actix_web::{HttpRequest, HttpResponse, Json, Responder};
use crate::error::MyError;
use sha1::Sha1;
use std::collections::HashMap;
use std::fs;
use std::io::prelude::*;
use std::path::PathBuf;
use crate::vcs::*;
use walkdir::WalkDir;
use xdg::BaseDirectories;

#[derive(Serialize, Debug)]
struct Book {
    files: HashMap<String, File>,
    location: PathBuf,
    name: String,
    remotes: Vec<String>,
    branches: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
enum Genre {
    Fantasy,
    Fiction,
    Academic,
}

#[allow(unused)]
#[derive(Serialize, Deserialize, Debug)]
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
    fn new(new_book_req: &NewBookRequest) -> Result<Self, MyError> {
        let mut files = HashMap::new();
        let root = File::new(&new_book_req.name, "", format!("0"), true, false);
        let book = File::new("Book", "Book", root.id.clone(), true, false);
        let chap1 = File::new("chap1", "Book/chap1", book.id.clone(), true, false);
        let sec1 = File::new("sec1", "Book/chap1/sec1", chap1.id.clone(), false, false);
        files.insert(root.id.clone(), root);
        files.insert(book.id.clone(), book);
        files.insert(chap1.id.clone(), chap1);
        files.insert(sec1.id.clone(), sec1);

        let repo = git_init(&new_book_req.location)?;
        let remotes = git_get_remotes(&repo)?;
        let branches = git_get_branches(&repo)?;

        Ok(Book {
            files,
            location: new_book_req.location.clone(),
            name: new_book_req.name.to_string(),
            remotes,
            branches,
        })
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

        let repo = git2::Repository::open(&location)?;
        let remotes = git_get_remotes(&repo)?;
        let branches = git_get_branches(&repo)?;
        Ok(Book {
            files,
            location: location.clone(),
            name: book_name.to_string(),
            remotes,
            branches
        })
    }
}

pub fn new_book(info: Json<NewBookRequest>) -> Result<impl Responder, MyError> {
    let book = Book::new(&info)?;
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

#[derive(Serialize, Deserialize, Debug)]
pub struct NewFileRequest {
    parent_id: String,
    name: String,
    is_folder: bool,
    location: PathBuf,
    parent_rel_path: PathBuf,
}

pub fn new_file(info: Json<NewFileRequest>) -> Result<impl Responder, MyError> {
    let rel_path = &info.parent_rel_path.join(&info.name);
    let rel_path_str = rel_path.to_str().ok_or("Filename contains invalid utf-8")?;
    let id = Sha1::from(rel_path_str).digest().to_string();

    let is_research = rel_path.to_string_lossy().contains("Research");

    let content: Option<String>;
    if info.is_folder {
        fs::create_dir_all(&info.location.join(&rel_path))?;
        content = None;
    } else {
        fs::File::create(&info.location.join(&rel_path))?;
        content = Some("".to_string());
    }
    fs::File::create(&info.location.join(".collabook/synopsis").join(&id))?;
    let f = File {
        id,
        name: info.name.clone(),
        rel_path: rel_path.clone(),
        parent: info.parent_id.clone(),
        is_visible: true,
        is_folder: info.is_folder,
        is_research,
        content,
        synopsis: "".to_string(),
    };
    let ser_f = serde_json::to_string(&f)?;
    Ok(HttpResponse::Ok().body(ser_f))
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SaveFileRequest {
    rel_path: PathBuf,
    content: String,
    location: PathBuf,
}

pub fn save_file(info: Json<SaveFileRequest>) -> Result<impl Responder, MyError> {
    let mut file = fs::File::create(&info.location.join(&info.rel_path))?;
    file.write_all(info.content.as_bytes())?;
    Ok(HttpResponse::Ok())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SaveSynopsisRequest {
    location: PathBuf,
    synopsis: String,
    id: String,
}

pub fn save_synopsis(info: Json<SaveSynopsisRequest>) -> Result<impl Responder, MyError> {
    let mut file = fs::File::create(&info.location.join(".collabook/synopsis").join(&info.id))?;
    file.write_all(info.synopsis.as_bytes())?;
    Ok(HttpResponse::Ok())
}

#[derive(Deserialize, Debug)]
pub struct DeleteFileRequest {
    location: PathBuf,
    rel_path: PathBuf,
    id: String,
}

pub fn delete_file(info: Json<DeleteFileRequest>) -> Result<impl Responder, MyError> {
    let path = &info.location.join(&info.rel_path);
    if path.is_dir() {
        fs::remove_dir_all(&path)?;
    } else {
        fs::remove_file(&path)?;
    }
    fs::remove_file(&info.location.join(".collabook/synopsis").join(&info.id))?;
    Ok("Deleted file".to_string())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Save {
    content: String,
    file: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type", content = "args")]
pub enum AuthType {
    Plain { user: String, pass: String },
    SSHAgent,
    SSHPath { path: String },
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Author {
    pub name: String,
    pub email: String,
    pub auth: AuthType,
}

pub fn get_author(_req: &HttpRequest) -> Result<impl Responder, MyError> {
    let author = get_user_config()?;
    Ok(HttpResponse::Ok().json(author))
}

pub fn create_author(info: Json<Author>) -> Result<impl Responder, MyError> {
    let xdg_dirs = BaseDirectories::with_prefix("collabook")?;
    let path = xdg_dirs.place_config_file("Config.toml")?;
    let author = info.into_inner();
    let contents = toml::to_string(&author)?;
    let mut file = fs::File::create(path)?;
    file.write_all(contents.as_bytes())?;
    Ok(HttpResponse::Ok().finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{http, test::TestServer, App};
    use tempdir::TempDir;

    fn create_app() -> App {
        App::new().resource("/newbook", |r| r.with(new_book))
    }

    #[test]
    fn it_works() {
        let temp_dir = TempDir::new("test_data").unwrap();
        let mut srv = TestServer::with_factory(create_app);
        let request = srv
            .client(http::Method::POST, "/newbook")
            .json(NewBookRequest {
                name: "MyTestBook".to_string(),
                location: PathBuf::from(temp_dir.path()),
                genre: Genre::Fantasy,
            })
            .unwrap();
        let resp = srv.execute(request.send()).unwrap();
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    }
}
