use actix_web::{HttpResponse, Json, Responder, Result};
use chrono::prelude::*;
use git2::{
    build::CheckoutBuilder, Branch, BranchType, Index, IndexAddOption, Oid, PushOptions, Remote,
    RemoteCallbacks, Repository,
};
use std::ops::Deref;
use std::path;
use std::path::Path;
use std::path::PathBuf;
//use xdg::BaseDirectories;

use crate::book::*;
use crate::error::MyError;

pub struct BookRepo {
    repo: Repository,
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
    pub fn new<P: AsRef<Path>>(location: P) -> Result<Self, MyError> {
        Ok(BookRepo {
            repo: Repository::init(location)?,
        })
    }

    pub fn from_location<P: AsRef<Path>>(location: P) -> Result<Self, MyError> {
        Ok(BookRepo {
            repo: Repository::open(location)?,
        })
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
            Err(_) => Ok(self.commit(Some("HEAD"), &sig, &sig, msg.as_ref(), &tree, &[])?),
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

    pub fn _get_branches(&self) -> Result<Vec<String>, MyError> {
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
                return Ok(branch);
            }
        }
        Err(MyError("Could not find current branch".to_string()))
    }

    fn _add_remote<S: AsRef<str>>(&self, name: S, url: S) -> Result<Remote, MyError> {
        Ok(self.remote(name.as_ref(), url.as_ref())?)
    }

    pub fn _get_remotes(&self) -> Result<Vec<String>, MyError> {
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

        let mut rebase = self.rebase(
            Some(&branch_annotated_commit),
            Some(&upstream_annotated_commit),
            None,
            None,
        )?;

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
        let op_current_index = rebase
            .operation_current()
            .ok_or("Could not find current operation")?;
        let op = rebase
            .nth(op_current_index)
            .ok_or("Could not get current patch")?;
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

        self._rebase(&branch, &upstream)?;
        Ok(())
    }

    fn _push(&self, branch: &Branch, remote: &mut Remote) -> Result<(), MyError> {
        let branch_name = branch.name()?.ok_or("Branch name is invalid")?;
        let mut push_opts = PushOptions::new();
        let mut remote_callbacks = RemoteCallbacks::new();
        remote_callbacks.credentials(get_credentials_callback);
        push_opts.remote_callbacks(remote_callbacks);

        let push_ref = format!("refs/heads/{0}:refs/heads/{0}", branch_name);

        remote.push(&[&push_ref], Some(&mut push_opts))?;

        Ok(())
    }
}

fn get_credentials_callback(
    _user: &str,
    user_from_url: Option<&str>,
    _cred: git2::CredentialType,
) -> Result<git2::Cred, git2::Error> {

    let user: Author = Author::read_from_disk().map_err(|_| git2::Error::from_str("Config file not found"))?;

    match user.auth {
        AuthType::Plain { user, pass } => git2::Cred::userpass_plaintext(&user, &pass),
        AuthType::SSHAgent => git2::Cred::ssh_key_from_agent(user_from_url.unwrap_or("git")),
        AuthType::SSHPath { path } => {
            let path = path::Path::new(&path);
            git2::Cred::ssh_key(user_from_url.unwrap_or("git"), None, &path, None)
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CommitRequest<T: AsRef<Path> = PathBuf> {
    message: String,
    location: T,
}

pub fn commit_request(info: Json<CommitRequest>) -> Result<impl Responder, MyError> {
    let repo = BookRepo::from_location(&info.location)?;
    let author = Author::read_from_disk()?;
    repo._commit(&info.message, &author)?;
    Ok(HttpResponse::Ok())
}

#[derive(Serialize, Deserialize, Debug)]
struct Logs {
    oid: String,
    message: String,
    author: String,
    time: String,
}

pub fn log_request(info: Json<BookLocation>) -> Result<impl Responder, MyError> {
    let repo = BookRepo::from_location(&info.location)?;
    let logs = repo._log()?;
    Ok(HttpResponse::Ok().json(logs))
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CheckoutRequest<P: AsRef<Path> = PathBuf, S: AsRef<str> = String> {
    oid: S,
    location: P,
}

pub fn checkout_request(info: Json<CheckoutRequest>) -> Result<impl Responder, MyError> {
    let repo = BookRepo::from_location(&info.location)?;
    let oid = Oid::from_str(&info.oid)?;
    repo._checkout_commit(oid)?;
    Ok(HttpResponse::Ok())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SwitchBranchRequest<P: AsRef<Path> = PathBuf, S: AsRef<str> = String> {
    name: S,
    location: P,
}

pub fn switch_branch_request(info: Json<SwitchBranchRequest>) -> Result<impl Responder, MyError> {
    let repo = BookRepo::from_location(&info.location)?;
    repo._switch_branch(&info.name)?;
    Ok(HttpResponse::Ok())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CreateBranchRequest<P: AsRef<Path> = PathBuf, S: AsRef<str> = String> {
    name: S,
    location: P,
}

pub fn create_branch_request(info: Json<CreateBranchRequest>) -> Result<impl Responder, MyError> {
    let repo = BookRepo::from_location(&info.location)?;
    repo._create_branch(&info.name)?;
    Ok(HttpResponse::Ok())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RemoteAddRequest<P: AsRef<Path> = PathBuf, S: AsRef<str> = String> {
    location: P,
    name: S,
    url: S,
}

pub fn remote_add_request(info: Json<RemoteAddRequest>) -> Result<impl Responder, MyError> {
    let repo = BookRepo::from_location(&info.location)?;
    repo._add_remote(&info.name, &info.url)?;
    Ok(HttpResponse::Ok())
}

pub fn get_remote_request(info: Json<BookLocation>) -> Result<impl Responder, MyError> {
    let repo = BookRepo::from_location(&info.location)?;
    let remotes = repo._get_remotes()?;
    Ok(HttpResponse::Ok().json(remotes))
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PushRequest<P: AsRef<Path> = PathBuf, S: AsRef<str> = String> {
    location: P,
    name: S,
}

pub fn push_request(info: Json<PushRequest>) -> Result<impl Responder, MyError> {
    let repo = BookRepo::from_location(&info.location)?;
    let branch = repo._current_branch()?;
    let mut remote = repo.find_remote(&info.name)?;
    repo._push(&branch, &mut remote)?;
    Ok(HttpResponse::Ok())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RebaseRequest<P: AsRef<Path> = PathBuf, S: AsRef<str> = String> {
    location: P,
    name: S,
}

pub fn rebase_request(info: Json<RebaseRequest>) -> Result<impl Responder, MyError> {
    let repo = BookRepo::from_location(&info.location)?;
    let branch = repo._current_branch()?;
    let branch_name = branch.name()?.ok_or("Could not get current branch name")?;

    //we need to first sync remote before calling rebase
    repo.find_remote(info.name.as_ref())?
        .fetch(&[&branch_name], None, None)?;

    let upsream_ref_str = format!("refs/remotes/{}/{}", info.name, branch_name);
    let upstream_ref = repo.find_reference(&upsream_ref_str)?;
    let upstream = Branch::wrap(upstream_ref);

    //TODO: Give better error messages in case of conflicts
    repo._rebase(&branch, &upstream)?;
    Ok(HttpResponse::Ok())
}

pub fn rebase_continue_request(info: Json<BookLocation>) -> Result<impl Responder, MyError> {
    let repo = BookRepo::from_location(&info.location)?;
    repo._rebase_continue()?;
    Ok(HttpResponse::Ok())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PullRequest<P: AsRef<Path> = PathBuf, S: AsRef<str> = String> {
    location: P,
    name: S,
}

pub fn pull_request(info: Json<PullRequest>) -> Result<impl Responder, MyError> {
    let repo = BookRepo::from_location(&info.location)?;
    repo._pull(&info.name)?;
    Ok(HttpResponse::Ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempdir::TempDir;
    use std::fs;
    use std::io::prelude::*;

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
            auth: AuthType::SSHAgent,
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
            auth: AuthType::SSHAgent,
        };

        let _oid = repo._commit("test commit", &author).unwrap();

        let _branch = repo._create_branch("dev").unwrap();

        assert_eq!(
            repo.find_branch("dev", BranchType::Local).unwrap().name(),
            Ok(Some("dev"))
        );
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
            auth: AuthType::SSHAgent,
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
            auth: AuthType::SSHAgent,
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
            auth: AuthType::SSHAgent,
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
            auth: AuthType::SSHAgent,
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
            auth: AuthType::SSHAgent,
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
            auth: AuthType::SSHAgent,
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
            auth: AuthType::SSHAgent,
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
            auth: AuthType::SSHAgent,
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
        let mut f1 = fs::OpenOptions::new()
            .truncate(true)
            .write(true)
            .open(path.join("test.txt"))
            .unwrap();
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
