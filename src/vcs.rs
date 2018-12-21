use actix_web::{HttpResponse, Json, Responder, Result};
use chrono::prelude::*;
use git2::Repository;
use xdg::BaseDirectories;
use std::fs;
use std::io::prelude::*;
use std::path;

use book::*;

macro_rules! badrequest {
    ($expr:expr) => (match $expr {
        Ok(val) => val,
        Err(e) => return HttpResponse::BadRequest().body(format!("{}",e))
    });
}

macro_rules! git2_error {
    ($expr:expr) => (match $expr {
        Ok(val) => val,
        Err(e) => return Err(git2::Error::from_str(&format!("{}",e)))
    });
}

/*
 *
 * Git stuff
 *
 */

// may be we should do this automatically for all books
pub fn git_init(info: Json<BookLocation>) -> impl Responder {
    match Repository::init(&info.location) {
        Ok(_) => HttpResponse::Ok(),
        Err(_) => HttpResponse::BadRequest(),
    }
}

pub fn git_add(info: Json<BookLocation>) -> impl Responder {
    match Repository::open(&info.location) {
        Ok(repo) => {
            repo.index()
                .unwrap()
                .add_all(["*"].iter(), git2::IndexAddOption::empty(), None)
                .unwrap();
            repo.index().unwrap().write().unwrap();
            HttpResponse::Ok()
        }
        Err(_) => HttpResponse::BadRequest(),
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CommitRequest {
    message: String,
    location: String,
}

pub fn git_commit(info: Json<CommitRequest>) -> impl Responder {
    match Repository::open(&info.location) {
        Ok(repo) => {
            // git add -a
            //
            repo.index()
                .unwrap()
                .add_all(["*"].iter(), git2::IndexAddOption::empty(), None)
                .unwrap();
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
                }
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
                    repo.commit(
                        Some("HEAD"),
                        &signature,
                        &signature,
                        &info.message,
                        &tree,
                        &[&parent],
                    )
                    .unwrap();
                }
                // we should check if the error is regarding there being no head or not (initial
                // commit)
                Err(_) => {
                    repo.commit(
                        Some("HEAD"),
                        &signature,
                        &signature,
                        &info.message,
                        &tree,
                        &[],
                    )
                    .unwrap();
                }
            };

            HttpResponse::Ok()
        }
        Err(_) => HttpResponse::BadRequest(),
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct CustomCommit {
    oid: String,
    message: String,
    author: String,
    time: String,
}

pub fn git_log(info: Json<BookLocation>) -> impl Responder {
    if let Ok(repo) = Repository::open(&info.location) {
        let mut walk = repo.revwalk().unwrap();
        walk.push_head().unwrap(); // TODO: repo with no commits will raise an error here
        let oids: Vec<git2::Oid> = walk.by_ref().collect::<Result<Vec<_>, _>>().unwrap();

        let mut commits: Vec<CustomCommit> = Vec::new();
        for oid in oids {
            if let Ok(commit) = repo.find_commit(oid) {
                let naive_datetime = NaiveDateTime::from_timestamp(commit.time().seconds(), 0);
                let datetime: DateTime<Utc> = DateTime::from_utc(naive_datetime, Utc);
                let custom_commit = CustomCommit {
                    oid: oid.to_string(),
                    message: commit.message().unwrap_or("").to_string(),
                    author: commit.author().name().unwrap_or("").to_string(),
                    time: datetime.to_rfc2822(),
                };
                commits.push(custom_commit);
            }
        }
        HttpResponse::Ok().json(commits)
    } else {
        HttpResponse::BadRequest().finish()
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GitCheckoutRequest {
    oid: String,
    location: String,
}

pub fn git_checkout(info: Json<GitCheckoutRequest>) -> impl Responder {
    if let Ok(repo) = Repository::open(&info.location) {
        let commit_oid = git2::Oid::from_str(&info.oid).unwrap();
        let commit = repo.find_commit(commit_oid).unwrap();
        let tree = commit.tree().unwrap().into_object();
        let mut checkout_builder = git2::build::CheckoutBuilder::new();
        checkout_builder.force().use_ours(true);
        repo.checkout_tree(&tree, Some(&mut checkout_builder))
            .unwrap();
        HttpResponse::Ok()
    } else {
        HttpResponse::BadRequest()
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GitRemoteAddRequest {
    location: String,
    name: String,
    url: String,
}

pub fn git_remote_add(info: Json<GitRemoteAddRequest>) -> impl Responder {
    match Repository::open(&info.location) {
        Ok(repo) => {
            match repo.remote(&info.name, &info.url) {
                Ok(_) => HttpResponse::Ok().finish(),
                Err(e) => HttpResponse::BadRequest().body(e.message().to_string())
            }
        },
        Err(e) => HttpResponse::BadRequest().body(e.message().to_string())
    }
}


#[derive(Serialize, Deserialize, Debug)]
pub struct GitPushRequest {
    location: String,
    name: String,
}

fn get_current_branch(repo: &Repository) -> Result<String, git2::Error> {
    let branches = repo.branches(Some(git2::BranchType::Local))?;
    for res_branch in branches {
        let (branch, _) = res_branch?;
        if branch.is_head() {
            let name = branch.name()?;
            match name {
                Some(name) => return Ok(name.to_string()),
                None => return Err(git2::Error::from_str("Invalid utf-8 name for branch"))
            }
        }

    }
    Err(git2::Error::from_str("Could not find current branch"))
}


pub fn git_push(info: Json<GitPushRequest>) -> impl Responder {
    let repo = badrequest!(Repository::open(&info.location));

    let branch_name = badrequest!(get_current_branch(&repo));

    let mut remote = badrequest!(repo.find_remote(&info.name));

    let mut push_opts = git2::PushOptions::new();
    let mut remote_callbacks = git2::RemoteCallbacks::new();

    remote_callbacks.credentials(move |_user, user_from_url, _credtype| {

        let xdg_dir = git2_error!(BaseDirectories::with_prefix("collabook"));

        let config_option = xdg_dir.find_config_file("Config.toml");
        let config;
        match config_option {
            Some(c) => config = c,
            None => return Err(git2::Error::from_str("Could not find config file"))
        };


        let mut file = git2_error!(fs::File::open(config));
        let mut contents = String::new();
        git2_error!(file.read_to_string(&mut contents));

        let user: Author = git2_error!(toml::from_str(&contents));

        match user.auth {
            AuthType::Plain{ user, pass} => {
                git2::Cred::userpass_plaintext(&user, &pass)
            }
            AuthType::SSHAgent => {
                git2::Cred::ssh_key_from_agent(user_from_url.unwrap_or("git"))
            }
            AuthType::SSHPath{ path } => {
                let path = path::Path::new(&path);
                git2::Cred::ssh_key(user_from_url.unwrap_or("git"), None, &path, None)
            }
        }
    });

    push_opts.remote_callbacks(remote_callbacks);

    let push_ref = format!("refs/heads/{0}:refs/heads/{0}", branch_name);

    badrequest!(remote.push(&[&push_ref], Some(&mut push_opts)));
    HttpResponse::Ok().finish()
}
