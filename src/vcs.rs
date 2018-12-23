use actix_web::{HttpResponse, Json, Responder, Result};
use chrono::prelude::*;
use git2::Repository;
use std::fs;
use std::io::prelude::*;
use std::path;
use xdg::BaseDirectories;

use book::*;
use error::MyError;
use git2_error;

// may be we should do this automatically for all books
pub fn git_init(info: Json<BookLocation>) -> Result<impl Responder, MyError> {
    Repository::init(&info.location)?;
    Ok(HttpResponse::Ok())
}

pub fn git_add(info: Json<BookLocation>) -> Result<impl Responder, MyError> {
    let repo = Repository::open(&info.location)?;
    let mut index = repo.index()?;
    index.add_all(["*"].iter(), git2::IndexAddOption::empty(), None)?;
    index.write()?;
    Ok(HttpResponse::Ok())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CommitRequest {
    message: String,
    location: String,
}

pub fn git_commit(info: Json<CommitRequest>) -> Result<impl Responder, MyError> {
    let repo = Repository::open(&info.location)?;

    // git add -a
    //
    let mut index = repo.index()?;
    index.add_all(["*"].iter(), git2::IndexAddOption::empty(), None)?;
    index.write()?;

    // git commit -m "message"
    let xdg_dirs = BaseDirectories::with_prefix("collabook")?;
    let path = xdg_dirs
        .find_config_file("Config.toml")
        .ok_or("Config not found")?;
    let mut file = fs::File::open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    let author: Author = toml::from_str(&contents)?;
    let signature = git2::Signature::now(&author.name, &author.email)?;

    let id = index.write_tree()?;
    let tree = repo.find_tree(id)?;

    match repo.head() {
        Ok(head) => {
            let target = head.target().ok_or("Cannot get target from head")?;
            let parent = repo.find_commit(target)?;
            repo.commit(
                Some("HEAD"),
                &signature,
                &signature,
                &info.message,
                &tree,
                &[&parent],
            )?;
        }
        Err(_) => {
            repo.commit(
                Some("HEAD"),
                &signature,
                &signature,
                &info.message,
                &tree,
                &[],
            )?;
        }
    };

    Ok(HttpResponse::Ok())
}

#[derive(Serialize, Deserialize, Debug)]
struct CustomCommit {
    oid: String,
    message: String,
    author: String,
    time: String,
}

pub fn git_log(info: Json<BookLocation>) -> Result<impl Responder, MyError> {
    let repo = Repository::open(&info.location)?;
    let mut walk = repo.revwalk()?;

    walk.push_head()?;

    let oids: Vec<git2::Oid> = walk.by_ref().collect::<Result<Vec<_>, _>>()?;

    let mut commits: Vec<CustomCommit> = Vec::new();
    for oid in oids {
        let commit = repo.find_commit(oid)?;

        //TODO: figure out the timestamp thingy
        let naive_datetime = NaiveDateTime::from_timestamp(
            commit.time().seconds() + commit.time().offset_minutes() as i64 * 60,
            0,
        );
        let datetime: DateTime<Utc> = DateTime::from_utc(naive_datetime, Utc);
        let custom_commit = CustomCommit {
            oid: oid.to_string(),
            message: commit.message().unwrap_or("").to_string(),
            author: commit.author().name().unwrap_or("").to_string(),
            time: datetime.to_rfc2822(),
        };
        commits.push(custom_commit);
    }
    Ok(HttpResponse::Ok().json(commits))
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GitCheckoutRequest {
    oid: String,
    location: String,
}

pub fn git_checkout(info: Json<GitCheckoutRequest>) -> Result<impl Responder, MyError> {
    let repo = Repository::open(&info.location)?;
    let commit_oid = git2::Oid::from_str(&info.oid)?;
    let commit = repo.find_commit(commit_oid)?;
    let tree = commit.tree()?.into_object();
    let mut checkout_builder = git2::build::CheckoutBuilder::new();
    checkout_builder.force().use_ours(true);
    repo.checkout_tree(&tree, Some(&mut checkout_builder))?;
    Ok(HttpResponse::Ok())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GitRemoteAddRequest {
    location: String,
    name: String,
    url: String,
}

pub fn git_remote_add(info: Json<GitRemoteAddRequest>) -> Result<impl Responder, MyError> {
    let repo = Repository::open(&info.location)?;
    repo.remote(&info.name, &info.url)?;
    Ok(HttpResponse::Ok())
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
                None => return Err(git2::Error::from_str("Invalid utf-8 name for branch")),
            }
        }
    }
    Err(git2::Error::from_str("Could not find current branch"))
}

fn get_credentials_callback(
    _user: &str,
    user_from_url: Option<&str>,
    _cred: git2::CredentialType,
) -> Result<git2::Cred, git2::Error> {
    let xdg_dir = git2_error!(BaseDirectories::with_prefix("collabook"));

    let config_option = xdg_dir.find_config_file("Config.toml");
    let config;
    match config_option {
        Some(c) => config = c,
        None => return Err(git2::Error::from_str("Could not find config file")),
    };

    let mut file = git2_error!(fs::File::open(config));
    let mut contents = String::new();
    git2_error!(file.read_to_string(&mut contents));

    let user: Author = git2_error!(toml::from_str(&contents));

    match user.auth {
        AuthType::Plain { user, pass } => git2::Cred::userpass_plaintext(&user, &pass),
        AuthType::SSHAgent => git2::Cred::ssh_key_from_agent(user_from_url.unwrap_or("git")),
        AuthType::SSHPath { path } => {
            let path = path::Path::new(&path);
            git2::Cred::ssh_key(user_from_url.unwrap_or("git"), None, &path, None)
        }
    }
}

pub fn git_push(info: Json<GitPushRequest>) -> Result<impl Responder, MyError> {
    let repo = Repository::open(&info.location)?;

    let branch_name = get_current_branch(&repo)?;

    let mut remote = repo.find_remote(&info.name)?;

    let mut push_opts = git2::PushOptions::new();
    let mut remote_callbacks = git2::RemoteCallbacks::new();

    remote_callbacks.credentials(get_credentials_callback);

    push_opts.remote_callbacks(remote_callbacks);

    let push_ref = format!("refs/heads/{0}:refs/heads/{0}", branch_name);

    remote.push(&[&push_ref], Some(&mut push_opts))?;
    Ok(HttpResponse::Ok())
}
