//! Job identifiers: UUIDv7 minted by the daemon (`docs/contracts.md` §2.4).
//!
//! `uuid` is not a dependency of this crate, so the 16 random bytes come from
//! `getrandom(2)` through `libc`, and the v7 layout (48-bit milliseconds, version,
//! variant) is assembled here.

use crate::protocol;

pub fn mint_job_id() -> Result<String, cauce_core::JobError> {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| {
            protocol::internal(format!("el reloj del sistema está antes de 1970: {error}"))
        })?
        .as_millis() as u64;

    let mut random = [0_u8; 10];
    fill_random(&mut random)?;

    let mut bytes = [0_u8; 16];
    bytes[..6].copy_from_slice(&millis.to_be_bytes()[2..]);
    bytes[6] = 0x70 | (random[0] & 0x0f);
    bytes[7] = random[1];
    bytes[8] = 0x80 | (random[2] & 0x3f);
    bytes[9..].copy_from_slice(&random[3..]);

    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    ))
}

fn fill_random(buffer: &mut [u8]) -> Result<(), cauce_core::JobError> {
    let mut filled = 0;
    while filled < buffer.len() {
        // SAFETY: `getrandom` writes at most `len` bytes into a live slice.
        let read = unsafe {
            libc::getrandom(
                buffer[filled..].as_mut_ptr().cast(),
                buffer.len() - filled,
                0,
            )
        };
        if read > 0 {
            filled += read as usize;
            continue;
        }
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::Interrupted {
            return Err(protocol::internal(format!(
                "El sistema no pudo entregar entropía para el identificador del trabajo: {error}."
            )));
        }
    }
    Ok(())
}
