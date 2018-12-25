use actix_web::{HttpResponse, Json, Responder, Result};
use chrono::prelude::*;
use git2::Repository;
use std::fs;
use std::io::prelude::*;
use std::path;
use std::path::PathBuf;
use xdg::BaseDirectories;

use book::*;
use error::MyError;
use git2_error;

pub fn git_init(location: &PathBuf) -> Result<Repository, MyError> {
    Ok(Repository::init(&location)?)
}

pub fn git_add_all(repo: &Repository) -> Result<git2::Index, MyError> {
    let mut index = repo.index()?;
    index.add_all(["*"].iter(), git2::IndexAddOption::empty(), None)?;
    index.write()?;
    Ok(index)
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CommitRequest {
    message: String,
    location: String,
}

pub fn get_user_config() -> Result<Author, MyError> {
    let xdg_dirs = BaseDirectories::with_prefix("collabook")?;
    let path = xdg_dirs
        .find_config_file("Config.toml")
        .ok_or("Config not found")?;
    let mut file = fs::File::open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(toml::from_str(&contents)?)
}

pub fn git_commit(info: Json<CommitRequest>) -> Result<impl Responder, MyError> {
    let repo = Repository::open(&info.location)?;

    let mut index = git_add_all(&repo)?;

    // git commit -m "message"
    let author = get_user_config()?;
    let sig = git2::Signature::now(&author.name, &author.email)?;

    let id = index.write_tree()?;
    let tree = repo.find_tree(id)?;

    match repo.head() {
        Ok(head) => {
            let target = head.target().ok_or("Cannot get target from head")?;
            let parent = repo.find_commit(target)?;
            repo.commit(Some("HEAD"), &sig, &sig, &info.message, &tree, &[&parent])?;
        }
        Err(_) => {
            repo.commit(Some("HEAD"), &sig, &sig, &info.message, &tree, &[])?;
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
pub struct GitGetRemotesRequest {
    location: PathBuf,
}

pub fn git_get_remotes(repo: &Repository) -> Result<Vec<String>, MyError> {
    let mut remotes: Vec<String> = Vec::new();
    for remote in repo.remotes()?.iter() {
        if remote.is_some() {
            remotes.push(String::from(remote.unwrap()));
        }
    }
    Ok(remotes)
}

pub fn git_get_branches(repo: &Repository) -> Result<Vec<String>, MyError> {
    let mut final_val: Vec<String> = Vec::new();
    let mut branches = repo.branches(Some(git2::BranchType::Local))?;

    while let Some(Ok((branch, _))) = branches.next() {
        let name = branch.name()?.ok_or("Invalid utf-8")?;
        final_val.push(name.to_string());
    }

    Ok(final_val)
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GitSwitchBranch {
    name: String,
    location: PathBuf,
}

pub fn git_switch_branch(info: Json<GitSwitchBranch>) -> Result<impl Responder, MyError> {
    let repo = Repository::open(&info.location)?;
    repo.set_head(&format!("refs/heads/{}", info.name))?;
    Ok(HttpResponse::Ok())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GitCreateBranch {
    name: String,
    location: PathBuf,
}

pub fn git_create_branch(info: Json<GitCreateBranch>) -> Result<impl Responder, MyError> {
    let repo = Repository::open(&info.location)?;
    let commit_oid = repo.head()?.resolve()?.target().ok_or("Cannot get current latest commit OID")?;
    let commit = repo.find_commit(commit_oid)?;
    repo.branch(&info.name, &commit, true)?;
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

    //fetch and rebase before pushing to remote
    //TODO fetch and merge will error if branch has not been pushed to remote yet
    //figure out if this is the case.
    //ignoring result for now
    remote.fetch(&[&branch_name], None, None)?;
    git_rebase(&repo, &info.name, &branch_name);

    remote.push(&[&push_ref], Some(&mut push_opts))?;

    Ok(HttpResponse::Ok())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GitPullRequest {
    location: PathBuf,
    name: String,
}

pub fn git_pull(info: Json<GitPullRequest>) -> Result<impl Responder, MyError> {
    let repo = Repository::open(&info.location)?;
    let current_branch = get_current_branch(&repo)?;
    repo.find_remote(&info.name)?
        .fetch(&[&current_branch], None, None)?;
    git_rebase(&repo, &info.name, &current_branch)?;
    Ok(HttpResponse::Ok())
}

pub fn git_rebase(
    repo: &Repository,
    remote_name: &str,
    current_branch: &str,
) -> Result<(), MyError> {
    let ref_branch = repo.find_reference(&format!("refs/heads/{}", current_branch))?;
    let branch = repo.reference_to_annotated_commit(&ref_branch)?;

    let ref_upstream =
        repo.find_reference(&format!("refs/remotes/{}/{}", remote_name, current_branch))?;
    let upstream = repo.reference_to_annotated_commit(&ref_upstream)?;

    let mut rebase = repo.rebase(Some(&branch), Some(&upstream), None, None)?;

    while let Some(Ok(patch)) = rebase.next() {
        let commit = repo.find_commit(patch.id())?;
        let message = commit.message().unwrap_or("");
        let sig = commit.author();
        rebase.commit(&sig, &sig, message)?;
    }

    //not sure why this is needed should probably use author config data here
    let random_sig = git2::Signature::now("rebaseauthor", "rebasemail")?;
    rebase.finish(&random_sig)?;
    Ok(())
}
