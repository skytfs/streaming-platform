use streaming_platform::tokio;

#[derive(Debug)]
pub enum Error {
	None,
	ConversionFromSlice(std::array::TryFromSliceError),
	CustomError(String),
	Io(std::io::Error),
	Chrono(chrono::format::ParseError),
	SdDeserializationFailed(rkyv::Unreachable),
	SerdeJson(serde_json::Error),
	Sled(sled::Error),
	SledTransaction(sled::transaction::TransactionError<()>),
	SledUnabortableTransaction(sled::transaction::UnabortableTransactionError),
	SledTransactionDc(sled::transaction::TransactionError<String>),
	StreamingPlatform(streaming_platform::ProcessError),
	SendFrame,
	IncorrectKeyInRequest
}

impl Error {
	pub fn custom(e: &str) -> Error {
		Error::CustomError(e.to_owned())
	}
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {		
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for Error {
    fn description(&self) -> &str {
        "I'm the superhero of errors"
    }    
}

impl From<std::array::TryFromSliceError> for Error {
	fn from(e: std::array::TryFromSliceError) -> Error {
		Error::ConversionFromSlice(e)
	}
}

impl From<String> for Error {
	fn from(e: String) -> Error {
		Error::CustomError(e)
	}
}

impl From<std::io::Error> for Error {
	fn from(e: std::io::Error) -> Error {
		Error::Io(e)
	}
}

impl From<chrono::format::ParseError> for Error {
	fn from(e: chrono::format::ParseError) -> Error {
		Error::Chrono(e)
	}
}

impl From<rkyv::Unreachable> for Error {
	fn from(e: rkyv::Unreachable) -> Error {
		Error::SdDeserializationFailed(e)
	}
}

impl From<serde_json::Error> for Error {
	fn from(e: serde_json::Error) -> Error {
		Error::SerdeJson(e)
	}
}

impl From<sled::Error> for Error {
	fn from(e: sled::Error) -> Error {
		Error::Sled(e)
	}
}

impl From<sled::transaction::TransactionError<()>> for Error {
	fn from(e: sled::transaction::TransactionError<()>) -> Error {
		Error::SledTransaction(e)
	}
}

impl From<sled::transaction::UnabortableTransactionError> for Error {
	fn from(e: sled::transaction::UnabortableTransactionError) -> Error {
		Error::SledUnabortableTransaction(e)
	}
}

impl From<streaming_platform::ProcessError> for Error {
    fn from(e: streaming_platform::ProcessError) -> Error {
        Error::StreamingPlatform(e)
    }
}

impl From<tokio::sync::mpsc::error::SendError<streaming_platform::Frame>> for Error {
    fn from(_: tokio::sync::mpsc::error::SendError<streaming_platform::Frame>) -> Error {
        Error::SendFrame
    }
}