use protocol::protocol_error::PrtError;
use std::error::Error;
use serde;
use std::fmt;


/// The errors that can arise while deserializing a ResultSet into a standard rust type/struct/Vec
pub enum DeserError {
    CustomError(String),
    ProgramError(String),
    UnknownField(String),
    MissingField(String),
    WrongValueType(String),
    TrailingRows,
    TrailingCols,
    FetchError(PrtError),
}
impl Error for DeserError {
    fn description(&self) -> &str {
        match *self {
            DeserError::CustomError(_) => "general error from the deserialization framework",
            DeserError::ProgramError(_) => {
                "error in the implementation of the resultset deserialization"
            }
            DeserError::UnknownField(_) => {
                "the target structure misses a field for which data are provided"
            }
            DeserError::MissingField(_) => {
                "the target structure contains a field for which no data are provided"
            }
            DeserError::WrongValueType(_) => "value types do not match",
            DeserError::TrailingRows => "trailing rows",
            DeserError::TrailingCols => "trailing columns",
            DeserError::FetchError(_) => "fetching more resultset lines or lob chunks failed",
        }
    }
}

pub fn prog_err(s: &str) -> DeserError {
    DeserError::ProgramError(String::from(s))
}

impl From<PrtError> for DeserError {
    fn from(error: PrtError) -> DeserError {
        DeserError::FetchError(error)
    }
}

impl serde::de::Error for DeserError {
    fn custom<T: Into<String>>(msg: T) -> Self {
        DeserError::CustomError(msg.into())
    }

    fn end_of_stream() -> DeserError {
        DeserError::TrailingRows
    }
    fn unknown_field(field: &str) -> DeserError {
        DeserError::UnknownField(String::from(field))
    }
    fn missing_field(field: &str) -> DeserError {
        DeserError::MissingField(String::from(field))
    }
}
impl fmt::Debug for DeserError {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            DeserError::CustomError(ref s) |
            DeserError::ProgramError(ref s) |
            DeserError::UnknownField(ref s) |
            DeserError::WrongValueType(ref s) => {
                write!(formatter, "{}: (\"{}\")", self.description(), s)
            }
            DeserError::MissingField(ref s) => {
                write!(formatter,
                       "{} (\"{}\"); note that the field mapping is case-sensitive, and partial \
                        deserialization is not supported",
                       self.description(),
                       s)
            }
            DeserError::TrailingRows | DeserError::TrailingCols => {
                write!(formatter, "{}", self.description())
            }
            DeserError::FetchError(ref error) => write!(formatter, "{:?}", error),
        }
    }
}
impl fmt::Display for DeserError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            DeserError::CustomError(ref s) |
            DeserError::ProgramError(ref s) |
            DeserError::UnknownField(ref s) |
            DeserError::MissingField(ref s) |
            DeserError::WrongValueType(ref s) => write!(fmt, "{} ", s),
            DeserError::TrailingRows => write!(fmt, "{} ", "TrailingRows"),
            DeserError::TrailingCols => write!(fmt, "{} ", "TrailingCols"),
            DeserError::FetchError(ref error) => write!(fmt, "{}", error),
        }
    }
}
pub type DeserResult<T> = Result<T, DeserError>;
