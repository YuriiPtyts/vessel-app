use serde::Serialize;

pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: &'static str,
    pub message: String,
    pub retryable: bool,
}

impl AppError {
    pub fn new(code: &'static str, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code,
            message: message.into(),
            retryable,
        }
    }

    pub fn internal() -> Self {
        Self::new(
            "internal",
            "The player could not complete this operation. Restart Vessel and try again.",
            true,
        )
    }

    pub fn cancelled() -> Self {
        Self::new("cancelled", "Stream resolution was cancelled.", true)
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for AppError {}
