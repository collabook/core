use crate::error::MyError;
use crate::vcs::*;
use crate::AppState;
use actix_web::{
    AsyncResponder, FutureResponse, HttpRequest, HttpResponse, Json, Responder, State,
};
use app_dirs::{AppDataType, AppInfo};
use futures::Future;
use sha1::Sha1;
use std::collections::HashMap;
use std::fs;
use std::io::prelude::*;
use std::path::Path;
use std::path::PathBuf;
use walkdir::WalkDir;

#[derive(Serialize, Debug)]
pub struct Book {
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
pub struct NewBookRequest<T: AsRef<Path>> {
    location: T,
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
    fn new(name: &str, rel_path: &str, parent: &str, is_folder: bool, is_research: bool) -> Self {
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

    fn from_location<P: AsRef<Path>>(path: P, book: P) -> Result<Self, MyError> {
        let name = &path
            .as_ref()
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("Filename contains invalid utf-8")?;

        let rel_path = path.as_ref().strip_prefix(&book)?;

        let parent_id = match rel_path.parent() {
            Some(parent) => parent
                .to_str()
                .ok_or("Filename contains invalid utf-8")
                .map(|par| Sha1::from(par).digest().to_string()),
            None => Ok("0".to_string()),
        }?;

        let rel_path_str = rel_path.to_str().ok_or("Filename contains invalid utf-8")?;
        let is_folder = path.as_ref().is_dir();
        let is_research = rel_path.to_string_lossy().contains("Research");

        //read contents
        let mut content: Option<String>;
        if !path.as_ref().is_dir() {
            let mut data = String::new();
            let mut file = fs::File::open(&path)?;
            file.read_to_string(&mut data)?;
            content = Some(data)
        } else {
            content = None
        }

        //read synopsis
        //TODO: should it be error if synopsis file is not found?
        //
        let rel_path_str2 = rel_path_str.replace("\\", "/"); //needed in windows as windows uses `\` instead of `/` on windows
        let id = Sha1::from(rel_path_str2).digest().to_string();
        //TODO: create synopsis file if not present
        let mut syn_file = fs::File::open(&book.as_ref().join(".collabook/synopsis").join(&id))?;
        let mut synopsis = String::new();
        syn_file.read_to_string(&mut synopsis)?;

        let f = File {
            id,
            name: name.to_string(),
            rel_path: PathBuf::from(rel_path.clone()),
            parent: parent_id,
            is_visible: true,
            is_folder,
            is_research,
            content,
            synopsis,
        };
        Ok(f)
    }
}

fn is_hidden(entry: &walkdir::DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| s.starts_with(".") || s.contains("target"))
        .unwrap_or(false)
}

impl Book {
    fn new<P: AsRef<Path>>(new_book_req: &NewBookRequest<P>) -> Result<Self, MyError> {
        /// This function handles the new book requests from the front end.
        /// The stuct `NewBookRequest` has fields:
        /// 1) location to save Book
        /// 2) name of the Book
        /// 3) book type
        /// Steps involved:
        /// 1) Create a HashMap mapping file ids and file objects
        let mut files = HashMap::new();

        /// 2) Create File objects
        let root = File::new(&new_book_req.name, "", "0", true, false);
        let book = File::new("Book", "Book", &root.id, true, false);
        let chap1 = File::new("Chap1", "Book/Chap1", &book.id, true, false);
        let sec1 = File::new("Sec1", "Book/Chap1/Sec1", &chap1.id, false, false);
        let research = File::new("Research", "Research", &root.id, true, true);
        let chars = File::new("Chars", "Research/Chars", &research.id, false, true);
        let world = File::new("World", "Research/World", &research.id, false, true);

        /// 3) Insert files into HashMap
        files.insert(root.id.clone(), root);
        files.insert(book.id.clone(), book);
        files.insert(chap1.id.clone(), chap1);
        files.insert(sec1.id.clone(), sec1);
        files.insert(research.id.clone(), research);
        files.insert(chars.id.clone(), chars);
        files.insert(world.id.clone(), world);

        //should this be done here or using other request?
        /// 4) Initialize repository inside the book directory
        let repo = BookRepo::new(&new_book_req.location)?;
        let remotes = repo._get_remotes()?;
        let branches = repo._get_branches()?;

        Ok(Book {
            files,
            location: new_book_req.location.as_ref().to_path_buf(),
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

    pub fn open(location: &Path) -> Result<Self, MyError> {
        /// Function handles opening of previously created books.
        /// Parameter `location` indicates the location of the book to open.
        /// Steps:
        //TODO: should ignore target folder
        /// 1) Create Hashmap for storing files
        let mut files: HashMap<String, File> = HashMap::new();

        //check if is a collabook directory
        //
        /// 2) Check if location is actully a Collabook directory or Not
        if !&location.join(".collabook").exists() {
            Err("Not a Collabook directory".to_string())?
        }

        let book_name = location
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("Filename contains invalid utf-8")?;

        //read files from disk
        /// 3) Recursively walk directories of the book adding files to the hashmap
        for entry in WalkDir::new(&location)
            .into_iter()
            .filter_entry(|e| !is_hidden(e))
            .filter_map(|e| e.ok())
        {
            let f = File::from_location(entry.path(), &location)?;
            files.insert(f.id.clone(), f);
        }

        //TODO: this should be provided as a parameter.. again not sure.
        /// 4) Get all remotes, branches of the book and send response to frontend
        let repo = BookRepo::from_location(&location)?;
        let remotes = repo._get_remotes()?;
        let branches = repo._get_branches()?;

        Ok(Book {
            files,
            location: location.to_path_buf(),
            name: book_name.to_string(),
            remotes,
            branches,
        })
    }

    pub fn combine_content<S: AsRef<str>>(&self, ids: &[S]) -> Result<String, MyError> {
        let mut content = String::new();
        for id in ids {
            let file = self.files.get(id.as_ref()).ok_or("File doesn't exist")?;
            let c = file.content.clone().ok_or("Is a directory")?;
            content.push_str(&c);
        }
        Ok(content)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CompileBookRequest<P: AsRef<Path> = PathBuf, S: AsRef<str> = String> {
    pub location: P,
    pub ids: Vec<S>,
}

pub fn compile_book(
    (state, info): (State<AppState>, Json<CompileBookRequest>),
) -> FutureResponse<HttpResponse> {
    state
        .compiler
        .send(info.into_inner())
        .from_err()
        .and_then(|res| match res {
            Ok(()) => Ok(HttpResponse::Ok().into()),
            Err(e) => Ok(HttpResponse::InternalServerError().body(e.to_string())),
        })
        .responder()
}

pub fn new_book<P: AsRef<Path>>(info: Json<NewBookRequest<P>>) -> Result<impl Responder, MyError> {
    //TODO: New book constructor shouldn't look for git remotes and branches itself, it should be a parameter of of constructor
    let book = Book::new(&info.into_inner())?;
    book.mkdirs()?;
    let ser_book = serde_json::to_string(&book)?;
    Ok(HttpResponse::Ok().body(ser_book))
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BookLocation<T: AsRef<Path> = PathBuf> {
    pub location: T,
}

pub fn open_book(info: Json<BookLocation<PathBuf>>) -> Result<impl Responder, MyError> {
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
    /// Handles new file creation requests
    /// `NewFileRequest` struct contains:
    /// 1) Parent folder of the new file to be created
    /// 2) Name of the file
    /// 3) location of the book
    /// 4) Parent directories relative path
    /// Steps involved
    ///
    ///
    /// 1) Calculate the SHA1 id from the relative path of the new file
    let rel_path = &info.parent_rel_path.join(&info.name);
    let rel_path_str = rel_path.to_str().ok_or("Filename contains invalid utf-8")?;
    let id = Sha1::from(rel_path_str).digest().to_string();

    /// 2) Check if file is part of the reseach content
    let is_research = rel_path.to_string_lossy().contains("Research");

    let content: Option<String>;
    /// 4) Create a synopsis file for the new file
    if info.is_folder {
        fs::create_dir_all(&info.location.join(&rel_path))?;
        content = None;
    } else {
        fs::File::create(&info.location.join(&rel_path))?;
        content = Some("".to_string());
    }
    /// 3) Create the file
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
    /// 5) Send the file info to front end
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

const APP_INFO: AppInfo = AppInfo {
    name: "Collabook",
    author: "Akhil",
};

#[derive(Serialize, Deserialize, Debug)]
pub struct Author {
    pub name: String,
    pub email: String,
    pub token: String,
    pub auth: AuthType,
}

impl crate::github::AccessToken for Author {
    fn get(&self) -> &str {
        &self.token
    }
}

impl Author {
    /// These functions are used for saving and loading config from and to disk respectively
    pub fn read_from_disk() -> Result<Self, MyError> {
        let path = app_dirs::app_root(AppDataType::UserConfig, &APP_INFO)?;
        let mut file = fs::File::open(path.join("Config.toml"))?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        Ok(toml::from_str(&contents)?)
    }

    pub fn write_to_disk(&self) -> Result<(), MyError> {
        let contents = toml::to_string(self)?;
        let path = app_dirs::app_root(AppDataType::UserConfig, &APP_INFO)?;
        let mut file = fs::File::create(&path.join("Config.toml"))?;
        file.write_all(contents.as_bytes())?;
        Ok(())
    }
}

pub fn get_author(_req: &HttpRequest<crate::AppState>) -> Result<impl Responder, MyError> {
    let author = Author::read_from_disk()?;
    Ok(HttpResponse::Ok().json(author))
}

pub fn create_author(info: Json<Author>) -> Result<impl Responder, MyError> {
    let author = info.into_inner();
    author.write_to_disk()?;
    Ok(HttpResponse::Ok().finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempdir::TempDir;

    #[test]
    fn test_config_file() {
        const APP_INFO: AppInfo = AppInfo {
            name: "Collabook",
            author: "Akhil",
        };

        let temp_dir = TempDir::new("test_dir").unwrap();
        let path = temp_dir.path();
        let author = Author {
            name: "akhil".to_string(),
            email: "email".to_string(),
            token: "token".to_string(),
            auth: AuthType::SSHAgent,
        };

        if cfg!(target_os = "linux") {
            std::env::set_var("HOME", path.join("test_dir"));
        } else if cfg!(target_os = "macos") {
            std::env::set_var("HOME", path.join("test_dir"));
        } else {
            std::env::set_var("APPDATA", path.join("test_dir"));
        }
        author.write_to_disk().unwrap();
        let path2 = app_dirs::get_app_root(AppDataType::UserConfig, &APP_INFO).unwrap();
        assert_eq!(path2.join("Config.toml").exists(), true);

        assert!(Author::read_from_disk().is_ok());
    }

    #[test]
    fn file_from_location() {
        let temp_dir = TempDir::new("test_book").unwrap();
        let path = temp_dir.path();

        //create a sec1 file and its synopsis file
        fs::File::create(path.join("Sec1")).unwrap();
        fs::create_dir_all(path.join(".collabook/synopsis")).unwrap();
        fs::File::create(
            path.join(".collabook/synopsis")
                .join("fbd662164e6d85d890952881f948ef17acaecc2d"),
        )
        .unwrap();

        let sec1 =
            File::from_location::<&Path>(&temp_dir.path().join("Sec1"), temp_dir.path()).unwrap();
        assert_eq!(sec1.is_folder, false);
        assert_eq!(sec1.content, Some("".to_string()));
        assert_eq!(sec1.name, "Sec1".to_string());
        assert_eq!(sec1.synopsis, "".to_string());

        //create a research file
        fs::create_dir_all(path.join("Research")).unwrap();
        let mut f = fs::File::create(path.join("Research/Chars")).unwrap();
        fs::File::create(
            path.join(".collabook/synopsis")
                .join("f7de51de1cd3ad2e789300bd2f11f84f9f35ced0"),
        )
        .unwrap();
        //add some content to Research/Chars file
        f.write_all(b"some content").unwrap();

        let chars = File::from_location::<&Path>(&path.join("Research/Chars"), path).unwrap();
        assert_eq!(chars.is_folder, false);
        assert_eq!(chars.is_research, true);
        assert_eq!(chars.content, Some("some content".to_string()));
    }

    #[test]
    fn test_file_constructor() {
        let root = File::new("testbook", "", "0", true, false);
        assert_eq!(&root.id, "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(root.content.is_none(), true);
        assert_eq!(&root.parent, "0");
        assert_eq!(&root.rel_path, Path::new(""));

        let sec1 = File::new("sec1", "Book/Chap1/Sec1", &root.id, false, false);
        assert_eq!(&sec1.id, "0ad0fd5d1787ebf9465fb46c743d35eb6b9ab783");
        assert_eq!(&sec1.content.unwrap(), "");
    }

    #[test]
    fn new_book_creates_correct_files() {
        let temp_dir = TempDir::new("test_dir").unwrap();
        let path = temp_dir.path().join("test_book");
        let req = Json(NewBookRequest {
            name: "test_book".to_string(),
            location: &path,
            genre: Genre::Fantasy,
        });
        new_book(req).unwrap();
        assert_eq!(path.join("Book/Chap1/Sec1").exists(), true);
        assert_eq!(path.join("Research/Chars").exists(), true);
    }

    #[test]
    fn open_book_reads_content_correctly() {
        let temp_dir = TempDir::new("test_dir").unwrap();
        let path = temp_dir.path().join("test_book");

        let root_sha1 = "da39a3ee5e6b4b0d3255bfef95601890afd80709";
        let book_sha1 = "f69f233005f15802770fd26fbf7ead52ec13d9e6";
        let research_sha1 = "be601df25eea91eaaf0d5e80263930143af345be";
        let sec1_sha1 = "169a91e9a0699ef3d8cee8f29a76856498ef0c0e";
        let chars_sha1 = "f7de51de1cd3ad2e789300bd2f11f84f9f35ced0";

        fs::create_dir_all(&path.join("Book")).unwrap();
        fs::create_dir_all(&path.join("Research")).unwrap();
        fs::create_dir_all(&path.join(".collabook/synopsis")).unwrap();
        let mut sec1 = fs::File::create(&path.join("Book/Sec1")).unwrap();

        fs::File::create(&path.join("Research/Chars")).unwrap();
        fs::File::create(&path.join(".collabook/synopsis").join(root_sha1)).unwrap();
        fs::File::create(&path.join(".collabook/synopsis").join(book_sha1)).unwrap();
        fs::File::create(&path.join(".collabook/synopsis").join(research_sha1)).unwrap();
        fs::File::create(&path.join(".collabook/synopsis").join(sec1_sha1)).unwrap();
        let mut chars =
            fs::File::create(&path.join(".collabook/synopsis").join(chars_sha1)).unwrap();

        let content = String::from("Synopsis for the character research file");
        let synopsis = String::from("Content inside the sec1 file");

        sec1.write_all(content.as_bytes()).unwrap();
        chars.write_all(synopsis.as_bytes()).unwrap();

        git2::Repository::init(&path).unwrap();

        let book = Book::open(&path).unwrap();

        assert_eq!(
            book.files
                .get("f7de51de1cd3ad2e789300bd2f11f84f9f35ced0")
                .unwrap()
                .synopsis,
            synopsis
        );
        assert_eq!(
            book.files
                .get("169a91e9a0699ef3d8cee8f29a76856498ef0c0e")
                .unwrap()
                .content,
            Some(content)
        );
    }

    #[test]
    #[should_panic(expected = "Not a Collabook directory")]
    fn opening_not_a_book_gives_error() {
        let req = Json(BookLocation {
            location: PathBuf::from("doesn't_exist"),
        });
        let book = open_book(req);
        book.unwrap();
    }

    #[test]
    #[ignore]
    #[should_panic(expected = "Invalid input")]
    fn create_author_with_empty_string() {
        let temp_dir = TempDir::new("test_dir").unwrap();
        let path = temp_dir.path().to_str().unwrap().to_owned();
        std::env::set_var("XDG_CONFIG_HOME", path);

        let author = Author {
            name: "".to_string(),
            email: "".to_string(),
            auth: AuthType::SSHAgent,
            token: "".to_string(),
        };
        author.write_to_disk().unwrap();
    }

    /*
    #[test]
    fn compile_book_works() {
        let temp_dir = TempDir::new("test_dir").unwrap();
        let path = temp_dir.path().join("test_book");
        let req = Json(NewBookRequest {
            name: "test_book".to_string(),
            location: &path,
            genre: Genre::Fantasy,
        });
        let book = Book::new(&req).unwrap();
        book.compile("<p>some content</p>").unwrap();
        assert!(path.join("target/book.pdf").exists());
    }
    */

    fn setup_book() -> (Book, TempDir) {
        let temp_dir = TempDir::new("test_dir").unwrap();
        let path = temp_dir.path().join("test_book");

        let mut files = HashMap::new();
        let root = File::new("test_book", "", "0", true, false);
        let book = File::new("Book", "Book", &root.id, true, false);
        let chap1 = File::new("Chap1", "Book/Chap1", &book.id, true, false);
        let sec1 = File::new("Sec1", "Book/Chap1/Sec1", &chap1.id, false, false);
        let sec2 = File::new("Sec2", "Book/Chap1/Sec2", &chap1.id, false, false);
        let chap2 = File::new("Chap2", "Book/Chap2", &book.id, true, false);
        let c2_sec1 = File::new("Sec1", "Book/Chap2/Sec1", &chap2.id, false, false);

        files.insert(root.id.clone(), root);
        files.insert(book.id.clone(), book);
        files.insert(chap1.id.clone(), chap1);
        files.insert(sec1.id.clone(), sec1);
        files.insert(sec2.id.clone(), sec2);
        files.insert(chap2.id.clone(), chap2);
        files.insert(c2_sec1.id.clone(), c2_sec1);

        let remotes = Vec::new();
        let branches = Vec::new();

        let book = Book {
            files,
            location: path,
            name: "test_book".to_string(),
            remotes,
            branches,
        };
        (book, temp_dir)
    }

    #[test]
    fn combine_content_works() {
        let (mut book, _) = setup_book();

        let ids = [
            "0ad0fd5d1787ebf9465fb46c743d35eb6b9ab783",
            "ad547798d6f4c6e1224226f5bd5253b93fde470f",
            "722935af1ff2d97062f48532f2ef95827da39b93",
        ];
        let mut c1_s1 = book.files.get_mut(ids[0]).unwrap();
        c1_s1.content = Some("hello from sec1\n".to_string());

        let mut c1_s2 = book.files.get_mut(ids[1]).unwrap();
        c1_s2.content = Some("hello from sec2\n".to_string());

        let mut c2_s1 = book.files.get_mut(ids[2]).unwrap();
        c2_s1.content = Some("hello from chap2 sec1".to_string());

        let content = book.combine_content(&ids).unwrap();
        assert_eq!(
            content,
            "hello from sec1\nhello from sec2\nhello from chap2 sec1".to_string()
        )
    }
    /*

    #[ignore]
    #[test]
    fn compile_book_api_works() {
        // test fails because content is empty
        let temp_dir = TempDir::new("test_dir").unwrap();
        let path = temp_dir.path().join("test_book");
        let req = Json(NewBookRequest {
            name: "test_book".to_string(),
            location: &path,
            genre: Genre::Fantasy,
        });

        new_book(req).unwrap();

        let ids = vec![
            "0ad0fd5d1787ebf9465fb46c743d35eb6b9ab783"
        ];

        let compile_request = Json(CompileBookRequest {
            location: &path,
            ids
        });

        assert!(compile_book(compile_request).is_ok());
    }
    */
}
