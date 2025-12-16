use libadep_cas::CasError;
use std::fmt::{Display, Formatter};
use tonic::Code;

#[derive(Debug, Clone)]
pub struct DepsdError {
    pub code: &'static str,
    pub message: String,
    pub status: Code,
}

impl DepsdError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            status: Code::Internal,
        }
    }

    pub fn with_status(mut self, status: Code) -> Self {
        self.status = status;
        self
    }

    pub fn into_anyhow(self) -> anyhow::Error {
        anyhow::Error::new(self)
    }

    pub fn from_cas_error(err: &CasError) -> Self {
        match err {
            CasError::CompressionRatioExceeded { raw, compressed, limit } => DepsdError::new(
                "E_ADEP_DEPS_COMPRESSION_LIMIT",
                format!(
                    "compression ratio exceeded: raw {raw} bytes vs compressed {compressed} bytes (limit {limit}x)"
                ),
            )
            .with_status(Code::InvalidArgument),
            CasError::ZipSlip { entry } => DepsdError::new(
                "E_ADEP_DEPS_ZIPSLIP",
                format!("archive entry escapes target directory: {entry}"),
            )
            .with_status(Code::InvalidArgument),
            other => DepsdError::new("E_ADEP_DEPS_CAS_ERROR", other.to_string())
                .with_status(Code::FailedPrecondition),
        }
    }
}

impl Display for DepsdError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for DepsdError {}

impl From<CasError> for DepsdError {
    fn from(err: CasError) -> Self {
        DepsdError::from_cas_error(&err)
    }
}
