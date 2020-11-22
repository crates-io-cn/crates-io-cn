use derive_more::{Display, Error, From};

#[derive(Debug, Display, From, Error)]
pub enum Error {
    IO(std::io::Error),
    Reqwest(reqwest::Error),
    Header(reqwest::header::ToStrError),
    SerdeJSON(serde_json::Error),
    Git2(git2::Error),
    EasyGit(easy_git::Error),
    MissingField,
    FetchFail,
    InvalidUTF8,
}
