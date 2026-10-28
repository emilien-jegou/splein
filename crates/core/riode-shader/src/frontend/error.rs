use std::{error::Error, fmt::Display};

#[derive(Debug)]
pub struct InternalError {
    pub error: String,
    pub please_report: bool,
}

impl Error for InternalError {}

impl Display for InternalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "an internal riode_shader error occured.")?;
        writeln!(f, "{}", self.error)?;

        if self.please_report {
            writeln!(
                f,
                "\nplease file an issue on github using this link:\n\
            https://github.com/raymarch/riode_shader/issues/new?title=Internal%20Error&body=Hi%2C%20I%20got%20the%20following%20internal%20error"
            )?;
        }
        Ok(())
    }
}

impl InternalError {
    pub fn new(please_report: bool, error: String) -> Self { Self { error, please_report } }
}
