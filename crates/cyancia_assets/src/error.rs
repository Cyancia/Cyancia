#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error(transparent)]
    Loader(Box<dyn std::error::Error>),
}

pub type Result<T> = std::result::Result<T, Error>;
