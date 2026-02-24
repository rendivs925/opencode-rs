use napi::Error;

#[derive(Debug)]
pub enum CoreError {
    Io(String),
    Glob(String),
    Token(String),
    Ignore(String),
    Archive(String),
    Cache(String),
    Watcher(String),
}

impl From<CoreError> for Error {
    fn from(err: CoreError) -> Self {
        match err {
            CoreError::Io(msg) => Error::from_reason(msg),
            CoreError::Glob(msg) => Error::from_reason(msg),
            CoreError::Token(msg) => Error::from_reason(msg),
            CoreError::Ignore(msg) => Error::from_reason(msg),
            CoreError::Archive(msg) => Error::from_reason(msg),
            CoreError::Cache(msg) => Error::from_reason(msg),
            CoreError::Watcher(msg) => Error::from_reason(msg),
        }
    }
}

impl CoreError {
    pub fn into_napi(self) -> Error {
        self.into()
    }
}

impl From<std::io::Error> for CoreError {
    fn from(err: std::io::Error) -> Self {
        CoreError::Io(err.to_string())
    }
}

impl From<notify::Error> for CoreError {
    fn from(err: notify::Error) -> Self {
        CoreError::Watcher(err.to_string())
    }
}

impl std::fmt::Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoreError::Io(msg) => write!(f, "IO error: {}", msg),
            CoreError::Glob(msg) => write!(f, "Glob error: {}", msg),
            CoreError::Token(msg) => write!(f, "Token error: {}", msg),
            CoreError::Ignore(msg) => write!(f, "Ignore error: {}", msg),
            CoreError::Archive(msg) => write!(f, "Archive error: {}", msg),
            CoreError::Cache(msg) => write!(f, "Cache error: {}", msg),
            CoreError::Watcher(msg) => write!(f, "Watcher error: {}", msg),
        }
    }
}

impl std::error::Error for CoreError {}
