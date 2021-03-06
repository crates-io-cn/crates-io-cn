use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Git2(#[from] git2::Error),
    #[error("symbolic reference not allowed")]
    SymbolicReference,
}
