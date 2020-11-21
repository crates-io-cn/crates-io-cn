use derive_more::{Display, Error, From};

#[derive(Debug, Display, From, Error)]
pub enum Error {
    Reqwest(reqwest::Error),
    Header(reqwest::header::ToStrError),
    MissingField,
    FetchFail,
}
