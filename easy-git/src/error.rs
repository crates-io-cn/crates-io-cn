use derive_more::{Display, Error, From};

#[derive(Debug, Display, From, Error)]
pub enum Error {
    Git2(git2::Error),
    BareRepo,
    SymbolicReference,
}
