#![allow(dead_code)]

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    Header(#[from] reqwest::header::ToStrError),
    #[error(transparent)]
    SerdeJSON(#[from] serde_json::Error),
    #[error(transparent)]
    Git2(#[from] git2::Error),
    #[error(transparent)]
    EasyGit(#[from] crate::easy_git::Error),
    #[error("missing field")]
    MissingField,
    #[error("fail to fetch")]
    FetchFail,
}
