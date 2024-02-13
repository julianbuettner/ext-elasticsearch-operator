use crate::elasticsearch::ElasticError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum OperatorError {
    #[error("{0}")]
    ElasticError(#[from] ElasticError),
    #[error("{0}")]
    KubeError(#[from] kube::Error),
}
