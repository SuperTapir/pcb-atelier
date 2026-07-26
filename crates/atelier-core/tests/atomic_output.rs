use std::io::Write;

use atelier_core::{AtomicWriteError, atomic_write_validated};

#[test]
fn validation_failure_keeps_previous_output_intact() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let destination = temp.path().join("output.bin");
    std::fs::write(&destination, b"previous").expect("write previous output");

    let error = atomic_write_validated(
        &destination,
        |file| {
            file.write_all(b"invalid")?;
            Ok::<_, std::io::Error>(())
        },
        |_temporary_path| Err::<(), _>("validation rejected output"),
    )
    .expect_err("validation must fail");

    assert!(matches!(error, AtomicWriteError::Validation(_)));
    assert_eq!(
        std::fs::read(&destination).expect("read preserved output"),
        b"previous"
    );
}

#[test]
fn validated_output_atomically_replaces_destination() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let destination = temp.path().join("output.bin");
    std::fs::write(&destination, b"previous").expect("write previous output");

    atomic_write_validated(
        &destination,
        |file| {
            file.write_all(b"validated")?;
            Ok::<_, std::io::Error>(())
        },
        |temporary_path| {
            let bytes = std::fs::read(temporary_path).map_err(|error| error.to_string())?;
            if bytes == b"validated" {
                Ok(())
            } else {
                Err("unexpected output".to_owned())
            }
        },
    )
    .expect("validated output must persist");

    assert_eq!(
        std::fs::read(&destination).expect("read replaced output"),
        b"validated"
    );
}
