#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("unknown command `{0}`")]
    UnknownCommand(String),
    #[error("bad parameter: {0}")]
    BadParam(String),
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Format(#[from] guitario_format::FormatError),
    #[error(transparent)]
    Midi(#[from] guitario_midi::MidiError),
    #[error(transparent)]
    Gp(#[from] guitario_gp::GpError),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}

impl EngineError {
    pub fn invalid(msg: impl Into<String>) -> EngineError {
        EngineError::Invalid(msg.into())
    }
}
