use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("the snapshot is damaged: {0}")]
    Corrupt(&'static str),
    #[error("exporting from Postgres failed")]
    Export,
    #[error(
        "usage: foryooru-server convert <snapshot> | serve <snapshot> [address] | simulate <snapshot> <report> <personas> <batches> | replay <snapshot> <reactions> <report> | replay-users <snapshot> <favourites.sqlite> <report>"
    )]
    Usage,
}

pub type Result<T> = std::result::Result<T, Error>;

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response()
    }
}
