extern crate actix_web;
#[macro_use]
extern crate serde_derive;
extern crate env_logger;
extern crate serde_json;
extern crate git2;
extern crate xdg;
extern crate toml;

use actix_web::middleware::{cors::Cors, Logger};
use actix_web::{http, server, App, Json, Path, Result, HttpResponse, Responder, HttpRequest};
use std::collections::HashMap;
use std::fs;
use std::io::prelude::*;
use std::path;
use git2::Repository;
use xdg::BaseDirectories;

fn mkdirs(tree: &Tree, location: &str, name: &str) -> Result<()> {
    for file in tree.0.values() {
        if file.is_folder {
            let mut path = path::PathBuf::from(location);

            if name != "" {
                path.push(name);
            };

            path.push(&file.full_path);
            fs::create_dir_all(path)?;
        }
    }

    for file in tree.0.values() {
        if !file.is_folder {
            let mut path = path::PathBuf::from(location);

            if name != "" {
                path.push(name);
            };

            path.push(&file.full_path);
            fs::File::create(path)?;
        }
    }

    Ok(())
}

#[derive(Serialize, Debug)]
struct Book {
    tree: Tree,
    content: Content,
    synopsis: Vec<Synopsis>,
    location: String,
    name: String,
}

#[allow(unused)]
#[derive(Deserialize)]
struct BookBuilder {
    location: String,
    name: String,
    genre: String,
}

impl BookBuilder {
    fn build(&self) -> Result<Book> {
        let tree = TreeBuilder::new()
            .name(&self.name)
            .location(&self.location)
            .genre(&self.genre)
            .build();

        let synopsis = Synopsis::from_tree(&tree)?;

        let content = Content::from_tree(&tree);

        Ok(Book {
            location: self.location.clone(),
            name: self.name.clone(),
            tree,
            content,
            synopsis,
        })
    }
}

struct TreeBuilder {
    name: Option<String>,
    location: Option<String>,
    genre: Option<String>,
}

impl TreeBuilder {
    fn new() -> TreeBuilder {
        TreeBuilder {
            name: None,
            location: None,
            genre: None,
        }
    }

    fn name(mut self, name: &str) -> TreeBuilder {
        self.name = Some(name.to_owned());
        self
    }

    fn location(mut self, location: &str) -> TreeBuilder {
        self.location = Some(location.to_owned());
        self
    }

    fn genre(mut self, genre: &str) -> TreeBuilder {
        self.genre = Some(genre.to_owned());
        self
    }

    fn build(self) -> Tree {
        Tree::from_builder(self)
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct Tree(HashMap<u32, File>);

impl Tree {
    fn from_builder(builder: TreeBuilder) -> Tree {
        let mut tree = HashMap::new();
        let name = builder.name.unwrap();

        let root = FileBuilder::new()
            .id(1)
            .name(&name)
            .full_path("")
            .parent(0)
            .visible(true)
            .folder(true)
            .finish();
        let book = FileBuilder::new()
            .id(2)
            .name("Book")
            .full_path("Book")
            .parent(1)
            .visible(true)
            .folder(true)
            .finish();
        let chap1 = FileBuilder::new()
            .id(3)
            .name("chap1")
            .full_path("Book/chap1")
            .parent(2)
            .visible(true)
            .folder(false)
            .finish();
        let chap2 = FileBuilder::new()
            .id(4)
            .name("chap2")
            .full_path("Book/chap2")
            .parent(2)
            .visible(true)
            .folder(false)
            .finish();
        let chap3 = FileBuilder::new()
            .id(5)
            .name("chap3")
            .full_path("Book/chap3")
            .parent(2)
            .visible(true)
            .folder(true)
            .finish();
        let sec1 = FileBuilder::new()
            .id(6)
            .name("sec1")
            .full_path("Book/chap3/sec1")
            .parent(5)
            .visible(true)
            .folder(false)
            .finish();

        let research = FileBuilder::new()
            .id(7)
            .name("Research")
            .full_path("Research")
            .parent(1)
            .visible(true)
            .folder(true)
            .research(true)
            .finish();
        let chars = FileBuilder::new()
            .id(8)
            .name("chars")
            .full_path("Research/chars")
            .parent(7)
            .visible(true)
            .folder(false)
            .research(true)
            .finish();
        let worlds = FileBuilder::new()
            .id(9)
            .name("worlds")
            .full_path("Research/worlds")
            .parent(7)
            .visible(true)
            .folder(false)
            .research(true)
            .finish();

        tree.insert(root.id, root);

        tree.insert(book.id, book);
        tree.insert(chap1.id, chap1);
        tree.insert(chap2.id, chap2);
        tree.insert(chap3.id, chap3);
        tree.insert(sec1.id, sec1);

        tree.insert(research.id, research);
        tree.insert(chars.id, chars);
        tree.insert(worlds.id, worlds);

        Tree(tree)
    }

    fn from_file(location: &str) -> Result<Tree> {
        let mut path = path::PathBuf::from(location);
        path.push("tree.json");
        let file = fs::File::open(&path)?;
        Ok(Tree(serde_json::from_reader(file)?))
    }

    fn to_disk(&self, location: &str) -> Result<()> {
        let mut path = path::PathBuf::from(location);
        path.push("tree.json");
        let mut file = fs::File::create(&path)?;
        let tree = serde_json::to_string(&self)?;
        file.write_all(tree.as_bytes())?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct File {
    id: u32,
    name: String,
    full_path: String,
    parent: u32,
    is_visible: bool,
    is_folder: bool,
    is_research: bool,
}

struct FileBuilder {
    id: Option<u32>,
    name: Option<String>,
    full_path: Option<String>,
    parent: Option<u32>,
    is_visible: Option<bool>,
    is_folder: Option<bool>,
    is_research: Option<bool>,
}

impl FileBuilder {
    fn new() -> Self {
        FileBuilder {
            id: None,
            name: None,
            full_path: None,
            parent: None,
            is_visible: None,
            is_folder: None,
            is_research: None,
        }
    }

    fn id(mut self, id: u32) -> Self {
        self.id = Some(id);
        self
    }

    fn name(mut self, name: &str) -> Self {
        self.name = Some(name.to_owned());
        self
    }

    fn full_path(mut self, path: &str) -> Self {
        self.full_path = Some(path.to_owned());
        self
    }

    fn parent(mut self, parent: u32) -> Self {
        self.parent = Some(parent);
        self
    }

    fn visible(mut self, visible: bool) -> Self {
        self.is_visible = Some(visible);
        self
    }

    fn folder(mut self, folder: bool) -> Self {
        self.is_folder = Some(folder);
        self
    }

    fn research(mut self, research: bool) -> Self {
        self.is_research = Some(research);
        self
    }

    fn finish(self) -> File {
        File {
            id: self.id.unwrap(),
            name: self.name.unwrap(),
            full_path: self.full_path.unwrap(),
            parent: self.parent.unwrap(),
            is_visible: self.is_visible.unwrap(),
            is_folder: self.is_folder.unwrap(),
            is_research: self.is_research.unwrap_or(false),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct Synopsis {
    id: u32,
    content: String,
}

impl Synopsis {
    fn from_tree(tree: &Tree) -> Result<Vec<Self>> {
        let mut vec_synopsis = Vec::new();
        for id in tree.0.keys() {
            vec_synopsis.push(Synopsis {
                id: *id,
                content: "".to_string(),
            });
        }
        Ok(vec_synopsis)
    }

    fn from_file(location: &str) -> Result<Vec<Self>> {
        let mut path = path::PathBuf::from(location);
        path.push("synopsis.json");
        let synopsis_file = fs::File::open(&path)?;
        let ser_synopsis = serde_json::from_reader(synopsis_file)?;
        Ok(ser_synopsis)
    }

    // vec synopsis and synopsis are not the same thing that is why this does not take self
    fn to_disk(synopsis: &[Synopsis], location: &str) -> Result<()> {
        let ser_synopsis = serde_json::to_string(synopsis)?;
        let mut path = path::PathBuf::from(location);
        path.push("synopsis.json");
        let mut file = fs::File::create(&path)?;
        file.write_all(ser_synopsis.as_bytes())?;
        Ok(())
    }
}

// should also create an empty hashmap for content
fn new_book(info: Json<BookBuilder>) -> Result<String> {
    let book = info.build()?;

    // must be a method of Book
    mkdirs(&book.tree, &book.location, &book.name)?;

    let mut path = path::PathBuf::from(&info.location);
    path.push(&info.name);

    // path is included here because name is not stored in book
    book.tree.to_disk(path.to_str().unwrap())?;

    Synopsis::to_disk(&book.synopsis, path.to_str().unwrap())?;

    Ok(serde_json::to_string(&book)?)
}

#[derive(Serialize, Deserialize, Debug)]
struct Content(HashMap<u32, String>);

impl Content {
    fn from_tree(tree: &Tree) -> Self {
        let mut content = HashMap::new();
        for (id, file) in &tree.0 {
            if file.is_folder == false {
                content.insert(id.clone(), String::new());
            }
        }
        Content(content)
    }

    fn from_disk(tree: &Tree, loc: &str) -> Result<Self> {
        let mut content = HashMap::new();
        for (id, file) in &tree.0 {
            if file.is_folder == false {
                let mut buf = String::new();

                let mut path = path::PathBuf::from(loc);
                path.push(&file.full_path);
                let mut f = fs::File::open(&path)?;
                f.read_to_string(&mut buf)?;
                content.insert(id.clone(), buf);
            }
        }
        Ok(Content(content))
    }

    fn to_disk(&self, tree: &Tree, loc: &str) -> Result<()> {
        for (id, file) in tree.0.iter() {
            if let Some(current_content) = self.0.get(id) {
                let location = format!("{}/{}", loc, file.full_path);
                let mut f = fs::File::create(location)?;
                f.write_all(current_content.as_bytes())?;
            }
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct BookLocation {
    // must contain bookname
    location: String,
}

fn open_book(info: Json<BookLocation>) -> Result<String> {
    let tree = Tree::from_file(&info.location)?;

    let synopsis = Synopsis::from_file(&info.location)?;

    let content = Content::from_disk(&tree, &info.location)?;

    let mut path = path::PathBuf::from(&info.location);
    let name = path.iter().last().unwrap().to_str().unwrap().to_owned(); //very ugly

    path.pop(); // remove book name
    let location = path.to_str().unwrap().to_owned();

    let res_data = Book {
        location,
        name,
        tree,
        content,
        synopsis,
    };

    Ok(serde_json::to_string(&res_data)?)
}

#[derive(Serialize, Deserialize, Debug)]
struct SaveBook {
    //location should include bookname
    location: String,
    tree: Tree,
    content: Option<Content>,
    synopsis: Option<Vec<Synopsis>>,
}

fn save_book(book: Json<SaveBook>) -> Result<String> {
    let mut path = path::PathBuf::from(&book.location);
    let book_name = path.file_name().unwrap().to_owned().into_string().unwrap();
    path.pop();

    mkdirs(&book.tree, &path.to_str().unwrap(), &book_name)?;

    book.tree.to_disk(&book.location)?;

    match &book.content {
        Some(content) => {
            content.to_disk(&book.tree, &book.location)?;
        }
        None => {}
    };

    match &book.synopsis {
        Some(synopsis) => {
            Synopsis::to_disk(&synopsis, &book.location)?;
        }
        None => {}
    };

    Ok("saved book".to_string())
}

fn delete_file(info: Path<(String,)>) -> Result<String> {
    fs::remove_dir_all(&info.0).unwrap();
    Ok("deleted file".to_string())
}

#[derive(Deserialize, Debug)]
struct Save {
    content: String,
    file: String,
}

fn save(info: Json<Save>) -> Result<String> {
    let mut f = fs::File::create(&info.file).unwrap();
    f.write_all(&info.content.as_bytes()).unwrap();
    Ok("save file".to_string())
}

#[derive(Serialize,Deserialize,Debug)]
struct Author {
    name: String,
    email: String,
}

fn get_author(_req: &HttpRequest) -> impl Responder {
    let xdg_dirs = BaseDirectories::with_prefix("collabook").unwrap();
    match xdg_dirs.find_config_file("Config.toml") {
        Some(config) => {
            let mut file = fs::File::open(config).unwrap();
            let mut contents = String::new();
            file.read_to_string(&mut contents).unwrap();
            let author: Author = toml::from_str(&contents).unwrap();
            HttpResponse::Ok().json(author)
        },
        None => {
            HttpResponse::NotFound().finish()
        },
    }
}

fn create_author(info: Json<Author>) -> impl Responder {
    let xdg_dirs = BaseDirectories::with_prefix("collabook").unwrap();
    match xdg_dirs.place_config_file("Config.toml") {
        Ok(path) => {
            let author = info.into_inner();
            let contents = toml::to_string(&author).unwrap();
            let mut file = fs::File::create(path).unwrap();
            file.write_all(contents.as_bytes()).unwrap();
            HttpResponse::Ok()
        },
        Err(_) => {
            HttpResponse::BadRequest()
        }
    }
}



/*
 *
 * Git stuff
 *
 */

// may be we should do this automatically for all books
fn git_init(info: Json<BookLocation>) -> impl Responder {
    let response = match Repository::init(&info.location) {
        Ok(_) => HttpResponse::Ok(),
        Err(_) => HttpResponse::BadRequest()
    };
    response
}

fn git_add(info: Json<BookLocation>) -> impl Responder {
    match Repository::open(&info.location) {
        Ok(repo) => {
            repo.index().unwrap().add_all(["*"].iter(), git2::IndexAddOption::empty(), None).unwrap();
            repo.index().unwrap().write().unwrap();
            HttpResponse::Ok()
        },
        Err(_) => HttpResponse::BadRequest()
    }
}

#[derive(Serialize,Deserialize,Debug)]
struct CommitRequest {
    message: String,
    location: String,
}

fn git_commit(info: Json<CommitRequest>) -> impl Responder  {
    match Repository::open(&info.location) {
        Ok(repo) => {

            // git add -a
            //
            repo.index().unwrap().add_all(["*"].iter(), git2::IndexAddOption::empty(), None).unwrap();
            repo.index().unwrap().write().unwrap();

            // git commit -m "message"
            let xdg_dirs = BaseDirectories::with_prefix("collabook").unwrap();
            let signature = match xdg_dirs.find_config_file("Config.toml") {
                Some(path) => {
                    let mut file = fs::File::open(path).unwrap();
                    let mut contents = String::new();
                    file.read_to_string(&mut contents).unwrap();
                    let author: Author = toml::from_str(&contents).unwrap();
                    git2::Signature::now(&author.name, &author.email).unwrap()
                },
                None => {
                    // should return error
                    git2::Signature::now("xyz", "xyz.com").unwrap()
                }
            };
            let mut index = repo.index().unwrap();
            let id = index.write_tree().unwrap();
            let tree = repo.find_tree(id).unwrap();
            // cannot figure out how to create initial commit
            match repo.head() {
                Ok(head) => {
                    let parent = repo.find_commit(head.target().unwrap()).unwrap();
                    repo.commit(Some("HEAD"), &signature, &signature, &info.message, &tree, &[&parent]).unwrap(); 
                },
                // we should check if the error is regarding there being no head or not (initial
                // commit)
                Err(_) => {
                    repo.commit(Some("HEAD"), &signature, &signature, &info.message, &tree, &[]).unwrap(); 
                }
            };

            HttpResponse::Ok()
        },
        Err(_) => {
            println!("sad");
            HttpResponse::BadRequest()
        }
    }
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
                .resource("/author", |r| {
                    r.method(http::Method::GET).f(get_author);
                    r.method(http::Method::POST).with(create_author);
                })
                .resource("/newbook", |r| r.method(http::Method::POST).with(new_book))
                .resource("/openbook", |r| {
                    r.method(http::Method::POST).with(open_book)
                })
                .resource("/savebook", |r| {
                    r.method(http::Method::POST).with(save_book)
                })
                .resource("/save", |r| r.method(http::Method::POST).with(save))
                .resource("/delete", |r| {
                    r.method(http::Method::POST).with(delete_file)
                })
                .resource("/gitinit", |r| r.method(http::Method::POST).with(git_init))
                .resource("/gitadd", |r| r.method(http::Method::POST).with(git_add))
                .resource("/gitcommit", |r| r.method(http::Method::POST).with(git_commit))
                .register()
        })
    })
    .bind("localhost:8088")
    .unwrap()
    .run();
}
