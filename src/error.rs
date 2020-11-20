use derive_more::{Display, From, Error};

#[derive(Debug, Display, From, Error)]
pub enum Error {
    Reqwest(reqwest::Error),
    Header(reqwest::header::ToStrError),
    MissingField
}