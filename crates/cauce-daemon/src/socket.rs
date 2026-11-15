//! Socket location, permissions, framing and peer credentials
//! (`docs/contracts.md` §5 and §9, `docs/architecture.md` §9 and §15).
//!
//! The socket lives in `$XDG_RUNTIME_DIR/cauce/`, the folder is `0700` and the
//! socket itself `0600`. A stale socket is only removed after proving that
//! nothing answers on it, so two daemons can never silently steal the path.

use std::io;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

use tokio::io::{AsyncBufRead, AsyncBufReadExt};
use tokio::net::{UnixListener, UnixStream};

const RUNTIME_SUBDIR: &str = "cauce";
const SOCKET_NAME: &str = "daemon.sock";
const DATABASE_NAME: &str = "cauce.db";
/// Only the owner may traverse the socket folder.
pub const DIR_MODE: u32 = 0o700;
/// Only the owner may connect.
pub const SOCKET_MODE: u32 = 0o600;

/// One decoded line, or a frame that blew past the limit.
#[derive(Debug, PartialEq, Eq)]
pub enum Frame {
    Line(Vec<u8>),
    TooLarge,
}

/// `$XDG_RUNTIME_DIR/cauce`, falling back to the temp dir when unset.
pub fn runtime_dir() -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(std::env::temp_dir);
    base.join(RUNTIME_SUBDIR)
}

/// `$XDG_RUNTIME_DIR/cauce/daemon.sock`.
pub fn socket_path() -> PathBuf {
    runtime_dir().join(SOCKET_NAME)
}

/// `$XDG_DATA_HOME/cauce`, defaulting to `$HOME/.local/share/cauce`.
pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".local/share"))
        })
        .unwrap_or_else(|| PathBuf::from("."));
    base.join(RUNTIME_SUBDIR)
}

/// `$XDG_DATA_HOME/cauce/cauce.db` (`docs/data-model.md` §1).
pub fn database_path() -> PathBuf {
    data_dir().join(DATABASE_NAME)
}

/// The authorized roots of v0.1: `$HOME`.
///
/// The portal grant of the desktop session (the exact folder the user picked in
/// the file chooser) is not representable in the protocol yet, so v0.1
/// authorizes the user's home and validates every path inside it
/// (`docs/contracts.md` §9). A portal-scoped grant is a later protocol field.
pub fn default_authorized_roots() -> Vec<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|home| home.is_absolute())
        .into_iter()
        .collect()
}

/// Creates `dir` with mode `0700` and tightens it if it already existed.
pub fn create_private_dir(dir: &Path) -> io::Result<()> {
    if !dir.exists() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(DIR_MODE)
            .create(dir)?;
    }
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(DIR_MODE))
}

/// Binds `path` as a `0600` socket, creating its folder as `0700` if needed.
///
/// A socket file left behind by a crash is unlinked only when connecting to it
/// fails, so a live daemon is never replaced.
pub async fn bind(path: &Path) -> io::Result<UnixListener> {
    if let Some(parent) = path.parent() {
        create_private_dir(parent)?;
    }
    remove_stale(path)?;
    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(SOCKET_MODE))?;
    Ok(listener)
}

fn remove_stale(path: &Path) -> io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    match std::os::unix::net::UnixStream::connect(path) {
        Ok(live) => {
            drop(live);
            Err(io::Error::new(
                io::ErrorKind::AddrInUse,
                format!(
                    "ya hay un daemon escuchando en {}: cerrá la instancia anterior antes de arrancar otra",
                    path.display()
                ),
            ))
        }
        Err(_) => std::fs::remove_file(path),
    }
}

/// Credentials of the peer on the other end of `stream` (`SO_PEERCRED`).
///
/// The kernel fills this in at `connect` time, so the daemon does not need the
/// client to identify itself: a different uid is closed before any response.
pub fn peer_uid(stream: &UnixStream) -> io::Result<u32> {
    let mut credentials = std::mem::MaybeUninit::<libc::ucred>::uninit();
    let mut length = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: the option is `SO_PEERCRED` on a connected Unix socket, which
    // writes exactly one `ucred` into the buffer and updates `length`.
    let result = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            credentials.as_mut_ptr().cast(),
            &mut length,
        )
    };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `getsockopt` succeeded, so the buffer is initialized.
    Ok(unsafe { credentials.assume_init() }.uid)
}

/// Effective uid the daemon runs as.
pub fn current_uid() -> u32 {
    // SAFETY: `geteuid` takes no arguments and cannot fail.
    unsafe { libc::geteuid() }
}

/// Reads one NDJSON frame into `buffer`, capped at [`MAX_MESSAGE_BYTES`].
///
/// `buffer` is owned by the caller on purpose: a cancelled read (the connection
/// also has to forward events) keeps every byte already consumed, so no partial
/// line is ever lost. `Ok(None)` means the peer closed the connection.
pub async fn read_frame<R>(reader: &mut R, buffer: &mut Vec<u8>) -> io::Result<Option<Frame>>
where
    R: AsyncBufRead + Unpin,
{
    use crate::protocol::MAX_MESSAGE_BYTES;

    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            if buffer.is_empty() {
                return Ok(None);
            }
            let line = std::mem::take(buffer);
            return Ok(Some(Frame::Line(line)));
        }

        match available.iter().position(|byte| *byte == b'\n') {
            Some(index) => {
                if buffer.len() + index > MAX_MESSAGE_BYTES {
                    reader.consume(index + 1);
                    return Ok(Some(Frame::TooLarge));
                }
                buffer.extend_from_slice(&available[..index]);
                reader.consume(index + 1);
                let mut line = std::mem::take(buffer);
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                return Ok(Some(Frame::Line(line)));
            }
            None => {
                let chunk = available.len();
                if buffer.len() + chunk > MAX_MESSAGE_BYTES {
                    reader.consume(chunk);
                    return Ok(Some(Frame::TooLarge));
                }
                buffer.extend_from_slice(available);
                reader.consume(chunk);
            }
        }
    }
}

/// Canonicalizes `raw` and checks it lives under one of `roots`.
///
/// `docs/contracts.md` §9: relative paths, NUL bytes and unreadable files are
/// `INVALID_INPUT`; a canonical path outside every root is `PATH_DENIED`. Roots
/// arrive canonicalized, so a symlink cannot escape an authorized folder.
pub fn authorize(roots: &[PathBuf], raw: &str) -> Result<String, cauce_core::JobError> {
    use cauce_core::{ErrorCode, JobError};

    let invalid = |message: String| JobError::new(ErrorCode::InvalidInput, message, false);
    if raw.trim().is_empty() || raw.contains('\0') {
        return Err(invalid(
            "La ruta está vacía o contiene bytes nulos; el daemon no la puede usar.".to_owned(),
        ));
    }

    let path = Path::new(raw);
    if !path.is_absolute() {
        return Err(invalid(format!(
            "La ruta `{raw}` es relativa: el daemon solo acepta rutas absolutas."
        )));
    }

    let canonical = path.canonicalize().map_err(|error| {
        invalid(format!(
            "La ruta `{raw}` no existe o no se puede leer: {error}."
        ))
    })?;
    let text = canonical.to_str().ok_or_else(|| {
        invalid("La ruta canonicalizada no es UTF-8 válido; el daemon la rechaza.".to_owned())
    })?;

    if !roots.iter().any(|root| canonical.starts_with(root)) {
        return Err(JobError::new(
            ErrorCode::PathDenied,
            format!(
                "La ruta `{text}` está fuera de las raíces autorizadas de Cauce; \
                 elegí el archivo desde la carpeta del usuario."
            ),
            false,
        ));
    }
    Ok(text.to_owned())
}
