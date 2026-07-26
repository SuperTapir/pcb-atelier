use std::fmt::{self, Display};
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

#[derive(Debug)]
pub enum AtomicWriteError<WriteError, ValidationError> {
    Io(io::Error),
    Write(WriteError),
    Validation(ValidationError),
    Persist(tempfile::PersistError),
}

impl<WriteError: Display, ValidationError: Display> Display
    for AtomicWriteError<WriteError, ValidationError>
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "failed to prepare atomic output: {error}"),
            Self::Write(error) => write!(formatter, "failed to write temporary output: {error}"),
            Self::Validation(error) => {
                write!(
                    formatter,
                    "temporary output did not pass validation: {error}"
                )
            }
            Self::Persist(error) => write!(formatter, "failed to replace destination: {error}"),
        }
    }
}

impl<WriteError, ValidationError> std::error::Error
    for AtomicWriteError<WriteError, ValidationError>
where
    WriteError: std::error::Error + 'static,
    ValidationError: std::error::Error + 'static,
{
}

/// Writes an output beside its destination, validates the completed temporary
/// file, and only then atomically replaces the destination.
pub fn atomic_write_validated<WriteError, ValidationError>(
    destination: &Path,
    write: impl FnOnce(&mut File) -> Result<(), WriteError>,
    validate: impl FnOnce(&Path) -> Result<(), ValidationError>,
) -> Result<(), AtomicWriteError<WriteError, ValidationError>> {
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty());
    let parent = match parent {
        Some(parent) => parent,
        None => Path::new("."),
    };

    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(AtomicWriteError::Io)?;
    write(temporary.as_file_mut()).map_err(AtomicWriteError::Write)?;
    temporary
        .as_file_mut()
        .flush()
        .map_err(AtomicWriteError::Io)?;
    temporary
        .as_file()
        .sync_all()
        .map_err(AtomicWriteError::Io)?;
    validate(temporary.path()).map_err(AtomicWriteError::Validation)?;
    temporary
        .persist(destination)
        .map_err(AtomicWriteError::Persist)?;
    Ok(())
}
