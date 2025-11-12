use ark_ec::hashing::HashToCurveError;

#[derive(Debug)]
pub enum IBEError {
    ExtractionError(String),
    DecryptionError(String),
    MapToCurveError(String),
    HashToCurveError(HashToCurveError)
}

impl std::fmt::Display for IBEError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            IBEError::ExtractionError(msg) => write!(f, "Extraction Error: {:?}", msg),
            IBEError::DecryptionError(msg) => write!(f, "Decryption Error: {:?}", msg),
            IBEError::MapToCurveError(msg) => write!(f, "MapToCurve Error: {:?}", msg),
            IBEError::HashToCurveError(msg) => write!(f, "HashToCurve Error: {:?}", msg)
        }
    }
}

impl std::error::Error for IBEError {}
impl From<HashToCurveError> for IBEError {
    fn from(error: HashToCurveError) -> Self {
        IBEError::HashToCurveError(error)
    }
}