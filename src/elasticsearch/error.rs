use thiserror::Error;

#[derive(Error, Debug)]
pub enum ElasticError {
    #[error("{0}")]
    HttpRequest(#[from] reqwest::Error),
    #[error("The provided credentials have been declined by ElasticSearch")]
    WrongCredentials,
    #[error("The provided login does work, but the user is missing the superuser credentials.")]
    NotSuperuser,
    #[error("An unexpected error occurred: {0}")]
    Custom(String),
}
