use git2::{Repository, Oid, Index, IndexAddOption, BranchType, Branch, Remote, build::CheckoutBuilder, PushOptions, RemoteCallbacks};
use std::ops::Deref;
use actix_web::{HttpResponse, Json, Responder, Result};
use chrono::prelude::*;
use std::fs;
use std::io::prelude::*;
use std::path;
use std::path::PathBuf;
use xdg::BaseDirectories;
use std::path::Path;

use crate::book::*;
use crate::error::MyError;
use crate::git2_error;

pub struct BookRepo {
    repo: Repository
}

impl Deref for BookRepo {
    type Target = Repository;

    fn deref(&self) -> &Repository {
        &self.repo
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct GitLog {
    oid: String,
    message: String,
    author: String,
    time: String,
}


impl BookRepo {
    fn new<P: AsRef<Path>>(location: P) -> Result<Self, MyError> {
        Ok(BookRepo {repo: Repository::init(location)?})
    }

    fn from_location<P: AsRef<Path>>(location: P) -> Result<Self, MyError> {
        Ok(BookRepo {repo: Repository::open(location)?})
    }

    // underscore is used so as not to be confused with with commit fn of git2::Repository::comit
    // method

    fn _add_all(&self) -> Result<Index, MyError> {
        let mut index = self.index()?;
        index.add_all(["*"].iter(), IndexAddOption::empty(), None)?;
        index.write()?;
        Ok(index)
    }

    fn _commit<S: AsRef<str>>(&self, msg: S, author: &Author) -> Result<Oid, MyError> {
        let mut index = self._add_all()?;
        let sig = git2::Signature::now(&author.name, &author.email)?;

        let id = index.write_tree()?;
        let tree = self.find_tree(id)?;

        match self.head() {
            Ok(head) => {
                let target = head.target().ok_or("Cannot get target from head")?;
                let parent = self.find_commit(target)?;
                Ok(self.commit(Some("HEAD"), &sig, &sig, msg.as_ref(), &tree, &[&parent])?)
            }
            Err(_) => {
                Ok(self.commit(Some("HEAD"), &sig, &sig, msg.as_ref(), &tree, &[])?)
            }
        }
    }

    //This will return error if repo doesn't contain any commits
    fn _create_branch(&self, name: impl AsRef<str>) -> Result<Branch, MyError> {
        let commit_oid = self
            .head()?
            .resolve()?
            .target()
            .ok_or("Cannot get current latest commit OID")?;

        let commit = self.find_commit(commit_oid)?;
        Ok(self.branch(name.as_ref(), &commit, true)?)
    }

    fn _switch_branch(&self, name: &str) -> Result<(), MyError> {
        let mut branch_ref = String::from("refs/heads/");
        branch_ref.push_str(name);
        self.set_head(&branch_ref)?;

        let mut checkout_builder = CheckoutBuilder::new();
        checkout_builder.force().use_ours(true);

        self.checkout_head(Some(&mut checkout_builder))?;

        Ok(())
    }

    fn _get_branches(&self) -> Result<Vec<String>, MyError> {
        let mut final_vec: Vec<String> = Vec::new();
        let mut branches = self.branches(Some(git2::BranchType::Local))?;

        while let Some(Ok((branch, _))) = branches.next() {
            let name = branch.name()?.ok_or("Invalid utf-8")?;
            final_vec.push(name.to_string());
        }

        Ok(final_vec)
    }

    fn _current_branch(&self) -> Result<Branch, MyError> {
        let branches = self.branches(Some(BranchType::Local))?;
        for branch in branches {
            let (branch, _) = branch?;
            if branch.is_head() {
                return Ok(branch)
            }
        }
        Err(MyError("Could not find current branch".to_string()))
    }

    fn _add_remote<S: AsRef<str>>(&self, name: S, url: S) -> Result<Remote, MyError> {
        Ok(self.remote(name.as_ref(), url.as_ref())?)
    }

    fn _get_remotes(&self) -> Result<Vec<String>, MyError> {
        let mut remotes: Vec<String> = Vec::new();
        for remote in self.remotes()?.iter() {
            if remote.is_some() {
                remotes.push(String::from(remote.unwrap())); //safe to uwnrap because of is_some check
            }
        }
        Ok(remotes)
    }

    fn _log(&self) -> Result<Vec<GitLog>, MyError> {

      let mut walk = self.revwalk()?;
  
      walk.push_head()?;
  
      let oids: Vec<git2::Oid> = walk.by_ref().collect::<Result<Vec<_>, _>>()?;
  
      let mut commits: Vec<GitLog> = Vec::new();
      for oid in oids {
          let commit = self.find_commit(oid)?;
  
          //TODO: figure out the timestamp thingy
          let naive_datetime = NaiveDateTime::from_timestamp(
              commit.time().seconds() + commit.time().offset_minutes() as i64 * 60,
              0,
          );
          let datetime: DateTime<Utc> = DateTime::from_utc(naive_datetime, Utc);
          let custom_commit = GitLog {
              oid: oid.to_string(),
              message: commit.message().unwrap_or("").to_string(),
              author: commit.author().name().unwrap_or("").to_string(),
              time: datetime.to_rfc2822(),
          };
          commits.push(custom_commit);
      }
      Ok(commits)
    }

    fn _checkout_commit(&self, oid: Oid) -> Result<(), MyError> {
        // let commit_oid = git2::Oid::from_str(oid)?; the request handler should perform this
        let commit = self.find_commit(oid)?;
        let tree = commit.tree()?.into_object();
        let mut checkout_builder = CheckoutBuilder::new();
        checkout_builder.force().use_ours(true);
        self.checkout_tree(&tree, Some(&mut checkout_builder))?;
        Ok(())
    }


    // `$ git rebase dev` here branch is the current branch and dev is the upstream branch.
    fn _rebase(&self, branch: &Branch, upstream: &Branch) -> Result<(), MyError> {

        let branch_annotated_commit = self.reference_to_annotated_commit(branch.get())?;
        let upstream_annotated_commit = self.reference_to_annotated_commit(upstream.get())?;
        
        let mut rebase = self.rebase(Some(&branch_annotated_commit), Some(&upstream_annotated_commit), None, None)?;

        while let Some(Ok(op)) = rebase.next() {
            let commit = self.find_commit(op.id())?;
            let msg = commit.message().unwrap_or("");
            let sig = commit.author();
            rebase.commit(&sig, &sig, msg)?;
        }
        let random_sig = git2::Signature::now("test", "test")?;
        rebase.finish(&random_sig)?;
        Ok(())
    }

    fn _rebase_continue(&self) -> Result<(), MyError> {

        self._add_all()?;
        let mut rebase = self.open_rebase(None)?;
        let op_current_index = rebase.operation_current().ok_or("Could not find current operation")?;
        let op = rebase.nth(op_current_index).ok_or("Could not get current patch")?;
        let commit = self.find_commit(op.id())?;
        let msg = commit.message().unwrap_or("");
        let sig = commit.author();
        rebase.commit(&sig, &sig, msg)?;


        while let Some(Ok(op)) = rebase.next() {
            let commit = self.find_commit(op.id())?;
            let msg = commit.message().unwrap_or("");
            let sig = commit.author();
            rebase.commit(&sig, &sig, msg)?;
        }

        //TODO: not sure why this is needed should probably use author config data here
        let random_sig = git2::Signature::now("rebaseauthor", "rebasemail")?;
        rebase.finish(&random_sig)?;;
        Ok(())
    }

    fn _pull(&self, from: impl AsRef<str>) -> Result<(), MyError> {
        let branch = self._current_branch()?;
        let branch_name = branch.name()?.ok_or("Could not get branch name")?;

        self.find_remote(from.as_ref())?
            .fetch(&[&branch_name], None, None)?;

        let upstream_ref_str = format!("refs/remotes/{}/{}", from.as_ref(), branch_name);
        let upstream_ref = self.find_reference(&upstream_ref_str)?;
        let upstream = Branch::wrap(upstream_ref);

        println!("{:?}", branch.name()?);

        self._rebase(&branch, &upstream)?;
        Ok(())
    }

    fn _push(&self, branch: &Branch, to: &mut Remote) -> Result<(), MyError> {

        let branch_name = branch.name()?.ok_or("Branch name is invalid")?;
        let mut push_opts = PushOptions::new();
        let mut remote_callbacks = RemoteCallbacks::new();
        remote_callbacks.credentials(get_credentials_callback);
        push_opts.remote_callbacks(remote_callbacks);

        let push_ref = format!("refs/heads/{0}:refs/heads/{0}", branch_name);

        to.push(&[&push_ref], Some(&mut push_opts))?;

        Ok(())
    }
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



//
//#[derive(Serialize, Deserialize, Debug)]
//pub struct CommitRequest<T: AsRef<Path>> {
//    message: String,
//    location: T,
//}
//
//
//pub fn git_commit<T: AsRef<Path>>(info: Json<CommitRequest<T>>) -> Result<impl Responder, MyError> {
//
//    if &info.message == "" {
//        return Err(MyError("Empty commit message".to_string()))
//    }
//
//    let repo = Repository::open(&info.location)?;
//
//    let mut index = git_add_all(&repo)?;
//
//    // git commit -m "message"
//    let author = Author::read_from_disk()?;
//    let sig = git2::Signature::now(&author.name, &author.email)?;
//
//    let id = index.write_tree()?;
//    let tree = repo.find_tree(id)?;
//
//    match repo.head() {
//        Ok(head) => {
//            let target = head.target().ok_or("Cannot get target from head")?;
//            let parent = repo.find_commit(target)?;
//            repo.commit(Some("HEAD"), &sig, &sig, &info.message, &tree, &[&parent])?;
//        }
//        Err(_) => {
//            repo.commit(Some("HEAD"), &sig, &sig, &info.message, &tree, &[])?;
//        }
//    };
//
//    Ok(HttpResponse::Ok())
//}
//
//#[derive(Serialize, Deserialize, Debug)]
//struct CustomCommit {
//    oid: String,
//    message: String,
//    author: String,
//    time: String,
//}
//
//pub fn git_log<T: AsRef<Path>>(info: Json<BookLocation<T>>) -> Result<impl Responder, MyError> {
//    let repo = Repository::open(&info.location)?;
//    let mut walk = repo.revwalk()?;
//
//    walk.push_head()?;
//
//    let oids: Vec<git2::Oid> = walk.by_ref().collect::<Result<Vec<_>, _>>()?;
//
//    let mut commits: Vec<CustomCommit> = Vec::new();
//    for oid in oids {
//        let commit = repo.find_commit(oid)?;
//
//        //TODO: figure out the timestamp thingy
//        let naive_datetime = NaiveDateTime::from_timestamp(
//            commit.time().seconds() + commit.time().offset_minutes() as i64 * 60,
//            0,
//        );
//        let datetime: DateTime<Utc> = DateTime::from_utc(naive_datetime, Utc);
//        let custom_commit = CustomCommit {
//            oid: oid.to_string(),
//            message: commit.message().unwrap_or("").to_string(),
//            author: commit.author().name().unwrap_or("").to_string(),
//            time: datetime.to_rfc2822(),
//        };
//        commits.push(custom_commit);
//    }
//    Ok(HttpResponse::Ok().json(commits))
//}
//
//#[derive(Serialize, Deserialize, Debug)]
//pub struct GitCheckoutRequest {
//    oid: String,
//    location: String,
//}
//
//pub fn git_checkout(info: Json<GitCheckoutRequest>) -> Result<impl Responder, MyError> {
//    let repo = Repository::open(&info.location)?;
//    let commit_oid = git2::Oid::from_str(&info.oid)?;
//    let commit = repo.find_commit(commit_oid)?;
//    let tree = commit.tree()?.into_object();
//    let mut checkout_builder = git2::build::CheckoutBuilder::new();
//    checkout_builder.force().use_ours(true);
//    repo.checkout_tree(&tree, Some(&mut checkout_builder))?;
//    Ok(HttpResponse::Ok())
//}
//


pub fn git_init(location: &PathBuf) -> Result<Repository, MyError> {
    Ok(Repository::init(&location)?)
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

//
//#[derive(Serialize, Deserialize, Debug)]
//pub struct GitSwitchBranch<P: AsRef<Path>>{
//    name: String,
//    location: P,
//}
//
//pub fn git_switch_branch<P: AsRef<Path>>(info: Json<GitSwitchBranch<P>>) -> Result<impl Responder, MyError> {
//    let repo = Repository::open(&info.location)?;
//    repo.set_head(&format!("refs/heads/{}", info.name))?;
//    Ok(HttpResponse::Ok())
//}
//
//#[derive(Serialize, Deserialize, Debug)]
//pub struct GitCreateBranch {
//    name: String,
//    location: PathBuf,
//}
//
//pub fn git_create_branch(info: Json<GitCreateBranch>) -> Result<impl Responder, MyError> {
//    let repo = Repository::open(&info.location)?;
//    let commit_oid = repo.head()?.resolve()?.target().ok_or("Cannot get current latest commit OID")?;
//    let commit = repo.find_commit(commit_oid)?;
//    repo.branch(&info.name, &commit, true)?;
//    Ok(HttpResponse::Ok())
//
//}
//
//#[derive(Serialize, Deserialize, Debug)]
//pub struct GitRemoteAddRequest {
//    location: String,
//    name: String,
//    url: String,
//}
//
//pub fn git_remote_add(info: Json<GitRemoteAddRequest>) -> Result<impl Responder, MyError> {
//    let repo = Repository::open(&info.location)?;
//    repo.remote(&info.name, &info.url)?;
//    Ok(HttpResponse::Ok())
//}
//
//#[derive(Serialize, Deserialize, Debug)]
//pub struct GitPushRequest {
//    location: String,
//    name: String,
//}
//
//fn get_current_branch(repo: &Repository) -> Result<String, git2::Error> {
//    let branches = repo.branches(Some(git2::BranchType::Local))?;
//    for res_branch in branches {
//        let (branch, _) = res_branch?;
//        if branch.is_head() {
//            let name = branch.name()?;
//            match name {
//                Some(name) => return Ok(name.to_string()),
//                None => return Err(git2::Error::from_str("Invalid utf-8 name for branch")),
//            }
//        }
//    }
//    Err(git2::Error::from_str("Could not find current branch"))
//}
//
//fn get_credentials_callback(
//    _user: &str,
//    user_from_url: Option<&str>,
//    _cred: git2::CredentialType,
//) -> Result<git2::Cred, git2::Error> {
//    let xdg_dir = git2_error!(BaseDirectories::with_prefix("collabook"));
//
//    let config_option = xdg_dir.find_config_file("Config.toml");
//    let config;
//    match config_option {
//        Some(c) => config = c,
//        None => return Err(git2::Error::from_str("Could not find config file")),
//    };
//
//    let mut file = git2_error!(fs::File::open(config));
//    let mut contents = String::new();
//    git2_error!(file.read_to_string(&mut contents));
//
//    let user: Author = git2_error!(toml::from_str(&contents));
//
//    match user.auth {
//        AuthType::Plain { user, pass } => git2::Cred::userpass_plaintext(&user, &pass),
//        AuthType::SSHAgent => git2::Cred::ssh_key_from_agent(user_from_url.unwrap_or("git")),
//        AuthType::SSHPath { path } => {
//            let path = path::Path::new(&path);
//            git2::Cred::ssh_key(user_from_url.unwrap_or("git"), None, &path, None)
//        }
//    }
//}
//
//pub fn git_push(info: Json<GitPushRequest>) -> Result<impl Responder, MyError> {
//    let repo = Repository::open(&info.location)?;
//
//    let branch_name = get_current_branch(&repo)?;
//
//    let mut remote = repo.find_remote(&info.name)?;
//
//    let mut push_opts = git2::PushOptions::new();
//    let mut remote_callbacks = git2::RemoteCallbacks::new();
//
//    remote_callbacks.credentials(get_credentials_callback);
//
//    push_opts.remote_callbacks(remote_callbacks);
//
//    let push_ref = format!("refs/heads/{0}:refs/heads/{0}", branch_name);
//
//    //fetch and rebase before pushing to remote
//    //TODO fetch and merge will error if branch has not been pushed to remote yet
//    //figure out if this is the case.
//    //ignoring result for now
//    remote.fetch(&[&branch_name], None, None)?;
//
//
//    match git_rebase(&repo, Some(&info.name), Some(&branch_name)) {
//        Err(ref e) if e.0 == "unstaged changes exist in workdir" => {
//            Err("A conflict seems to have arised. Resolve conflicts and click continue rebase")
//        },
//        Err(ref e) => Err(e.0.as_ref()),
//        _ => Ok(())
//    }?;
//
//    remote.push(&[&push_ref], Some(&mut push_opts))?;
//
//    Ok(HttpResponse::Ok())
//}
//
//#[derive(Serialize, Deserialize, Debug)]
//pub struct Location {
//    location: path::PathBuf,
//}
//
//
//pub fn git_rebase_continue(info: Json<Location>) -> Result<impl Responder, MyError> {
//    let repo = Repository::open(&info.location)?;
//    if repo.state() != git2::RepositoryState::RebaseMerge {
//        return Err(MyError("Rebase is not in progress".to_string()))
//    }
//    git_rebase(&repo, None, None)?;
//    Ok(HttpResponse::Ok())
//}
//
//#[derive(Serialize, Deserialize, Debug)]
//pub struct GitPullRequest {
//    location: PathBuf,
//    name: String,
//}
//
//pub fn git_pull(info: Json<GitPullRequest>) -> Result<impl Responder, MyError> {
//    let repo = Repository::open(&info.location)?;
//    let current_branch = get_current_branch(&repo)?;
//    repo.find_remote(&info.name)?
//        .fetch(&[&current_branch], None, None)?;
//    git_rebase(&repo, Some(&info.name), Some(&current_branch))?;
//    Ok(HttpResponse::Ok())
//}
//
/*
pub fn git_rebase(
    repo: &Repository,
    remote_name: Option<&str>,
    current_branch: Option<&str>,
) -> Result<(), MyError> {
    let mut rebase;
    match repo.state() {
        git2::RepositoryState::RebaseMerge => {
            git_add_all(&repo)?;
            rebase = repo.open_rebase(None)?;
            let op_current_index = rebase.operation_current().ok_or("Could not find current operation")?;
            let op = rebase.nth(op_current_index).ok_or("Could not get current patch")?;
            let commit = repo.find_commit(op.id())?;
            let msg = commit.message().unwrap_or("");
            let sig = commit.author();
            rebase.commit(&sig, &sig, msg)?;
        },
        _ => {
            let ref_branch = repo.find_reference(&format!("refs/heads/{}", current_branch.unwrap()))?;
            let branch = repo.reference_to_annotated_commit(&ref_branch)?;
            let ref_upstream = repo.find_reference(&format!("refs/remotes/{}/{}", remote_name.unwrap(), current_branch.unwrap()))?;
            let upstream = repo.reference_to_annotated_commit(&ref_upstream)?;
            rebase = repo.rebase(Some(&branch), Some(&upstream), None, None)?;
        }
    }


    while let Some(Ok(op)) = rebase.next() {
        let commit = repo.find_commit(op.id())?;
        let msg = commit.message().unwrap_or("");
        let sig = commit.author();
        rebase.commit(&sig, &sig, msg)?;
    }

    //TODO: not sure why this is needed should probably use author config data here
    let random_sig = git2::Signature::now("rebaseauthor", "rebasemail")?;
    rebase.finish(&random_sig)?;;
    Ok(())
}

*/

#[cfg(test)]
mod tests {
    use super::*;
    use tempdir::TempDir;

    #[test]
    fn test_commit() {
        let temp_dir = TempDir::new("test_dir").unwrap();
        let path = temp_dir.path();

        let repo = BookRepo::new(path).unwrap();

        let mut f = fs::File::create(&path.join("test.txt")).unwrap();
        f.write_all(b"some text").unwrap();

        let author = Author {
            name: "name".to_string(),
            email: "email".to_string(),
            auth: AuthType::SSHAgent
        };

        let oid = repo._commit("test commit", &author).unwrap();

        let commit = repo.find_commit(oid).unwrap();
        assert_eq!(commit.message().unwrap(), "test commit");
    }

    #[test]
    fn test_create_branch() {
        let temp_dir = TempDir::new("test_dir").unwrap();
        let path = temp_dir.path();

        let repo = BookRepo::new(path).unwrap();

        //this is an error as repo doesn't have any commits yet
        assert!(repo._create_branch("topic").is_err());


        let mut f = fs::File::create(&path.join("test.txt")).unwrap();
        f.write_all(b"some text").unwrap();

        let author = Author {
            name: "name".to_string(),
            email: "email".to_string(),
            auth: AuthType::SSHAgent
        };

        let _oid = repo._commit("test commit", &author).unwrap();

        let _branch = repo._create_branch("dev").unwrap();

        assert_eq!(repo.find_branch("dev", BranchType::Local).unwrap().name(), Ok(Some("dev")));
    }

    #[test]
    fn test_get_branches() {

        let temp_dir = TempDir::new("test_dir").unwrap();
        let path = temp_dir.path();

        let repo = BookRepo::new(path).unwrap();

        let mut f = fs::File::create(&path.join("test.txt")).unwrap();
        f.write_all(b"some text").unwrap();

        let author = Author {
            name: "name".to_string(),
            email: "email".to_string(),
            auth: AuthType::SSHAgent
        };

        let oid = repo._commit("test commit", &author).unwrap();
        repo.find_commit(oid).unwrap();

        repo._create_branch("dev").unwrap();
        repo._create_branch("topic").unwrap();

        let branches = repo._get_branches().unwrap();
        assert_eq!(branches.len(), 3);
    }

    #[test]
    fn test_switch_branch() {
        let temp_dir = TempDir::new("test_dir").unwrap();
        let path = temp_dir.path();

        let repo = BookRepo::new(path).unwrap();

        let mut f = fs::File::create(&path.join("test.txt")).unwrap();
        f.write_all(b"some text").unwrap();

        let author = Author {
            name: "name".to_string(),
            email: "email".to_string(),
            auth: AuthType::SSHAgent
        };

        let oid = repo._commit("test commit", &author).unwrap();
        repo.find_commit(oid).unwrap();

        repo._create_branch("topic").unwrap();
        repo._switch_branch("topic").unwrap();
        f.write_all(b"changes made on topic branch").unwrap();

        repo._switch_branch("master").unwrap();

        let content = fs::read_to_string(path.join("test.txt")).unwrap();
        assert_eq!(content, "some text");
    }

    #[test]
    fn test_add_remotes() {
        let temp_dir = TempDir::new("test_dir").unwrap();
        let path = temp_dir.path();

        let repo = BookRepo::new(path).unwrap();

        assert!(repo._add_remote("origin", "http://remote.git").is_ok());
    }

    #[test]
    fn test_get_remotes() {
        let temp_dir = TempDir::new("test_dir").unwrap();
        let path = temp_dir.path();

        let repo = BookRepo::new(path).unwrap();

        repo.remote("origin", "url1").unwrap();
        repo.remote("upstream", "url2").unwrap();

        assert_eq!(repo._get_remotes().unwrap().len(), 2);
    }

    #[test]
    fn test_log() {
        let temp_dir = TempDir::new("test_dir").unwrap();
        let path = temp_dir.path();

        let repo = BookRepo::new(path).unwrap();
        let author = Author {
            name: "name".to_string(),
            email: "email".to_string(),
            auth: AuthType::SSHAgent
        };

        let mut f = fs::File::create(&path.join("test.txt")).unwrap();
        f.write_all(b"some text").unwrap();
        let _oid1 = repo._commit("test commit 1", &author).unwrap();

        f.write_all(b"some other text").unwrap();
        let _oid2 = repo._commit("test commit 2 ", &author).unwrap();

        assert_eq!(repo._log().unwrap().len(), 2);
    }

    #[test]
    fn test_checkout_commit() {
        let temp_dir = TempDir::new("test_dir").unwrap();
        let path = temp_dir.path();

        let repo = BookRepo::new(path).unwrap();
        let author = Author {
            name: "name".to_string(),
            email: "email".to_string(),
            auth: AuthType::SSHAgent
        };

        let mut f = fs::File::create(&path.join("test.txt")).unwrap();
        f.write_all(b"some text").unwrap();
        let oid1 = repo._commit("test commit 1", &author).unwrap();

        f.write_all(b"some other text").unwrap();
        let _oid2 = repo._commit("test commit 2 ", &author).unwrap();

        repo._checkout_commit(oid1).unwrap();
        let content = fs::read_to_string(&path.join("test.txt")).unwrap();

        assert_eq!(content, "some text".to_string());
    }

    #[test]
    fn test_get_current_branch() {
        let temp_dir = TempDir::new("test_dir").unwrap();
        let path = temp_dir.path();

        let repo = BookRepo::new(path).unwrap();
        let author = Author {
            name: "name".to_string(),
            email: "email".to_string(),
            auth: AuthType::SSHAgent
        };

        let mut f = fs::File::create(&path.join("test.txt")).unwrap();
        f.write_all(b"some text").unwrap();
        let _oid1 = repo._commit("test commit 1", &author).unwrap();

        assert_eq!(repo._current_branch().unwrap().name(), Ok(Some("master")));

        repo._create_branch("topic").unwrap();
        repo._switch_branch("topic").unwrap();

        assert_eq!(repo._current_branch().unwrap().name(), Ok(Some("topic")));
   }

    #[test]
    fn test_rebase_no_conflict() {
        let temp_dir = TempDir::new("test_dir").unwrap();
        let path = temp_dir.path();

        let repo = BookRepo::new(&path).unwrap();
        let author = Author {
            name: "name".to_string(),
            email: "email".to_string(),
            auth: AuthType::SSHAgent
        };

        // create a commit common on both branches
        let mut f = fs::File::create(&path.join("test.txt")).unwrap();
        f.write_all(b"initial content").unwrap();
        repo._commit("initial commit", &author).unwrap();

        repo._create_branch("topic").unwrap();
        
        //add a commit to master branch
        f.write_all(b"our content").unwrap();
        repo._commit("our modifications", &author).unwrap();

        //add a non conflict commit to topic branch
        repo._switch_branch("topic").unwrap();
        let mut f2 = fs::File::create(&path.join("test2.txt")).unwrap();
        f2.write_all(b"their content").unwrap();
        repo._commit("their modifications", &author).unwrap();

        let branch = repo.find_branch("master", BranchType::Local).unwrap();
        let upstream = repo.find_branch("topic", BranchType::Local).unwrap();

        repo._switch_branch("master").unwrap();
        repo._rebase(&branch, &upstream).unwrap();

        assert_eq!(repo._log().unwrap().len(), 3);
    }

    #[test]
    fn test_rebase_conflict() {
        let temp_dir = TempDir::new("test_dir").unwrap();
        let path = temp_dir.path();

        let repo = BookRepo::new(&path).unwrap();
        let author = Author {
            name: "name".to_string(),
            email: "email".to_string(),
            auth: AuthType::SSHAgent
        };

        // create a commit common on both branches
        let mut f = fs::File::create(&path.join("test.txt")).unwrap();
        f.write_all(b"initial content").unwrap();
        repo._commit("initial commit", &author).unwrap();

        repo._create_branch("topic").unwrap();
        
        //add a commit to master branch
        f.write_all(b"our content").unwrap();
        repo._commit("our modifications", &author).unwrap();

        //add a conflict commit to topic branch
        repo._switch_branch("topic").unwrap();
        f.write_all(b"their content").unwrap();
        repo._commit("their modifications", &author).unwrap();

        let branch = repo.find_branch("master", BranchType::Local).unwrap();
        let upstream = repo.find_branch("topic", BranchType::Local).unwrap();

        repo._switch_branch("master").unwrap();
        assert!(repo._rebase(&branch, &upstream).is_err());
    }

    #[test]
    fn test_rebase_continue() {
        let temp_dir = TempDir::new("test_dir").unwrap();
        let path = temp_dir.path();

        let repo = BookRepo::new(&path).unwrap();
        let author = Author {
            name: "name".to_string(),
            email: "email".to_string(),
            auth: AuthType::SSHAgent
        };

        // create a commit common on both branches
        let mut f = fs::File::create(&path.join("test.txt")).unwrap();
        f.write_all(b"initial content").unwrap();
        repo._commit("initial commit", &author).unwrap();

        repo._create_branch("topic").unwrap();
        
        //add a commit to master branch
        f.write_all(b"\nour content").unwrap();
        repo._commit("our modifications", &author).unwrap();

        //add a conflicting commit to topic branch
        repo._switch_branch("topic").unwrap();
        f.write_all(b"\ntheir content").unwrap();
        repo._commit("their modifications", &author).unwrap();

        let branch = repo.find_branch("master", BranchType::Local).unwrap();
        let upstream = repo.find_branch("topic", BranchType::Local).unwrap();

        repo._switch_branch("master").unwrap();
        assert!(repo._rebase(&branch, &upstream).is_err());

        //resolve the conflicts
        let mut f1 = fs::OpenOptions::new().truncate(true).write(true).open(path.join("test.txt")).unwrap();
        f1.write_all(b"resolved conflict").unwrap();

        repo._rebase_continue().unwrap();

    }

    #[test]
    #[ignore]
    fn test_pull() {
        let path = PathBuf::from("/home/akhil/Videos/testbook5");

        let repo = BookRepo::from_location(&path).unwrap();
        repo._pull("origin").unwrap();
    }
}

    //#[test]
    //fn test_commit() {
    //    let temp_dir = TempDir::new("test_dir").unwrap();
    //    let path = temp_dir.path();

    //    let repo = Repository::init(&path).unwrap();
    //    let mut f = fs::File::create(&path.join("test.txt")).unwrap();
    //    f.write_all(b"some text").unwrap();
    //    let req = Json(CommitRequest {location: path, message: "test commit".to_string()});
    //    git_commit(req).unwrap();

    //    let head_oid = repo.head().unwrap().target().unwrap();
    //    let commit = repo.find_commit(head_oid).unwrap();
    //    assert_eq!(commit.message().unwrap(), "test commit");

    //    let req2 = Json(CommitRequest {location: path, message: "".to_string()});
    //    assert!(git_commit(req2).is_err());

    //}

//    #[test]
//    fn test_log() {
//        //difficut to get the inner value due to return type being impl trait responder
//
//        let temp_dir = TempDir::new("test_dir").unwrap();
//        let path = temp_dir.path();
//
//        let _repo = Repository::init(&path).unwrap();
//        let mut f = fs::File::create(&path.join("text.txt")).unwrap();
//        f.write_all(b"some text").unwrap();
//        let req = Json(CommitRequest {location: path, message: "1st commit".to_string()});
//        git_commit(req).unwrap();
//
//        let req = Json(BookLocation {location: path});
//        assert!(git_log(req).is_ok());
//    }
//
//    #[test]
//    fn test_swich_branch() {
//        let temp_dir = TempDir::new("test_dir").unwrap();
//        let path = temp_dir.path();
//
//        let _repo = Repository::init(&path).unwrap();
//
//        let req = Json(CommitRequest {location: path, message: "test commit".to_string()});
//        git_commit(req).unwrap();
//
//        assert!(git_switch_branch(Json(GitSwitchBranch { location: path, name: "master".to_string()})).is_ok());
//        assert!(git_switch_branch(Json(GitSwitchBranch { location: path, name: "doen't exit".to_string()})).is_err());
//
//    }
//
