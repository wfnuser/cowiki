use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("transport error: {0}")]
    Transport(String),

    #[error("HTTP status {0}")]
    HttpStatus(u16),

    #[error("protocol error: {0}")]
    Protocol(String),

    #[error("timeout")]
    Timeout,

    #[error("pool exhausted: no available agents for task type '{task_type}'")]
    PoolExhausted { task_type: String },

    #[error("harness not found for task type: {0}")]
    HarnessNotFound(String),

    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("invalid harness URL: {0}")]
    InvalidUrl(String),
}
