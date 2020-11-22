#[macro_use]
extern crate log;

use std::path::Path;
use git2::{Repository, Tree, ResetType, Oid};

mod error;
pub use error::Error;

pub trait EasyGit {
    fn add<P: AsRef<Path>>(&self, path: P) -> Result<Tree<'_>, Error>;
    fn reset_origin_hard(&self) -> Result<(), Error>;
    fn commit_message<M: AsRef<str>>(&self, message: M, tree: &Tree<'_>) -> Result<Oid, Error>;
    fn fetch_origin(&self) -> Result<(), Error>;
    fn rebase_master(&self) -> Result<(), Error>;
}

impl EasyGit for Repository {
    /// git add $path
    fn add<P: AsRef<Path>>(&self, path: P) -> Result<Tree<'_>, Error> {
        let mut index = self.index()?;
        index.add_path(path.as_ref())?;
        index.write()?;
        let tree_id = index.write_tree()?;
        Ok(self.find_tree(tree_id)?)
    }

    /// git reset --hard origin/HEAD
    fn reset_origin_hard(&self) -> Result<(), Error> {
        let remote = self.find_reference("refs/remotes/origin/HEAD")?;
        self.reset(remote.peel_to_commit()?.as_object(), ResetType::Hard, None)?;
        Ok(())
    }

    /// git commit -m $message
    fn commit_message<M: AsRef<str>>(&self, message: M, tree: &Tree<'_>) -> Result<Oid, Error> {
        let head = self.head()?;
        let head_oid = head.target().ok_or(Error::SymbolicReference)?;
        let parent = self.find_commit(head_oid)?;
        let sig = self.signature()?;
        let oid = self.commit(Some("HEAD"), &sig, &sig, message.as_ref(), &tree, &[&parent])?;
        Ok(oid)
    }

    /// git fetch origin/master
    fn fetch_origin(&self) -> Result<(), Error> {
        let mut origin = self.find_remote("origin")?;
        origin.fetch(&["master"], None, None)?;
        Ok(())
    }

    /// git rebase master origin/master
    fn rebase_master(&self) -> Result<(), Error> {
        let local = self.find_reference("refs/heads/master")?;
        let local_commit = self.reference_to_annotated_commit(&local)?;
        trace!("{:?} at: {:?}", local_commit.refname(), local_commit.id());
        let remote = self.find_reference("refs/remotes/origin/master")?;
        let remote_commit = self.reference_to_annotated_commit(&remote)?;
        trace!("{:?} at: {:?}", remote_commit.refname(), remote_commit.id());
        let mut rebase = self.rebase(Some(&local_commit), Some(&remote_commit), None, None)?;
        while let Some(r) = rebase.next() {
            let ro = r?;
            trace!("RebaseOperation: {:?} {:?}", ro.kind(), ro.id());
        }
        let oid = rebase.commit(None, &self.signature()?, None)?;
        trace!("Rebase commit at: {:?}", oid);
        rebase.finish(None)?;
        Ok(())
    }
}