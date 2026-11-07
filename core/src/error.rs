use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    /// Wrong password, or the data was modified. AES-GCM cannot tell these apart.
    Auth,
    /// The bytes are not a valid Arcaforty file.
    Format(&'static str),
    /// A field or file exceeds a size limit.
    Limit(&'static str),
    /// The password does not meet the rules.
    Password(&'static str),
    /// A file location the app refuses to use.
    Path(&'static str),
    Kdf,
    Io(std::io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Auth => f.write_str("Wrong password, or the file is damaged."),
            Error::Format(m) | Error::Limit(m) | Error::Password(m) | Error::Path(m) => {
                f.write_str(m)
            }
            Error::Kdf => f.write_str("Key derivation failed."),
            Error::Io(e) => write!(f, "File error: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}
