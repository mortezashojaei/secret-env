//! Read environment variables straight into [`memsafe::Secret`].

use std::{env::VarError, fmt, io, path::PathBuf, sync::atomic};

use memsafe::{Secret, error::MemoryError};

/// An environment variable held in locked, dump-excluded memory.
///
/// There is no `Display`, `Deref` or `Clone`; reach the value through
/// [`EnvSecret::with`] or [`EnvSecret::with_str`].
///
/// ```compile_fail
/// let secret = secret_env::require("PATH").unwrap();
/// println!("{secret}");
/// ```
///
/// ```compile_fail
/// let secret = secret_env::require("PATH").unwrap();
/// let _ = &*secret;
/// ```
///
/// ```compile_fail
/// let secret = secret_env::require("PATH").unwrap();
/// let _ = secret.clone();
/// ```
pub struct EnvSecret<const N: usize = 2048> {
    secret: memsafe::Secret<N>, // only place secret bytes live
    len: usize,
}

impl<const N: usize> EnvSecret<N> {
    /// Runs `f` on the value while the page is readable.
    ///
    /// Copies made by `f` live outside protected memory. Finish before the
    /// first `.await`.
    pub fn with<R>(&mut self, f: impl FnOnce(&[u8]) -> R) -> Result<R, Error> {
        let page = self.secret.read().map_err(Error::Memory)?;
        Ok(f(&page[..self.len]))
    }

    /// Like [`EnvSecret::with`], for the value as `&str`.
    pub fn with_str<R>(&mut self, f: impl FnOnce(&str) -> R) -> Result<R, Error> {
        self.with(|bytes| f(std::str::from_utf8(bytes).expect("copied from a String")))
    }
}

impl<const N: usize> fmt::Debug for EnvSecret<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EnvSecret").field("len", &self.len).finish()
    }
}

/// Why a variable could not be loaded. Never contains a value.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// The variable is not set.
    NotPresent(String),
    /// The variable is set but is not valid Unicode.
    NotUnicode(String),
    /// The value is `len` bytes, more than the `max` the secret can hold.
    TooLong { name: String, len: usize, max: usize },
    /// Allocating or reading protected memory failed.
    Memory(MemoryError),
    /// Loading `.env` failed. Parse errors omit the offending line.
    Dotenv(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotPresent(name) => write!(f, "{name} is not set"),
            Error::NotUnicode(name) => write!(f, "{name} is not valid Unicode"),
            Error::TooLong { name, len, max } => write!(f, "{name} is {len} bytes, max {max}"),
            Error::Memory(err) => err.fmt(f),
            Error::Dotenv(err) => write!(f, "failed to load .env: {err}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Memory(err) => Some(err),
            Error::Dotenv(err) => Some(err),
            _ => None,
        }
    }
}

/// Loads `.env` from the current directory or a parent, like
/// [`dotenvy::dotenv`]: existing variables are not overridden, and a
/// missing file is an error (silence it with `.ok()`).
///
/// Call this before starting threads; it writes the process environment.
pub fn dotenv() -> Result<PathBuf, Error> {
    dotenvy::dotenv().map_err(|err| match err {
        dotenvy::Error::Io(err) => Error::Dotenv(err),
        dotenvy::Error::LineParse(line, index) => {
            wipe(&mut line.into_bytes());
            Error::Dotenv(io::Error::other(format!("invalid line, error at index {index}")))
        }
        _ => Error::Dotenv(io::Error::other("invalid .env file")),
    })
}

/// Reads `name` into a 2048-byte secret, or `None` if it is not set.
pub fn var(name: &str) -> Result<Option<EnvSecret>, Error> {
    var_sized(name)
}

/// Reads `name` into a 2048-byte secret, failing if it is not set.
pub fn require(name: &str) -> Result<EnvSecret, Error> {
    require_sized(name)
}

/// Reads `name` into an `N`-byte secret, failing if it is not set.
pub fn require_sized<const N: usize>(name: &str) -> Result<EnvSecret<N>, Error> {
    var_sized(name)?.ok_or_else(|| Error::NotPresent(name.to_owned()))
}

/// Reads `name` into an `N`-byte secret, or `None` if it is not set.
///
/// The temporary `String` from [`std::env::var`] is zeroed, including spare
/// capacity. The process environment keeps its own copy.
pub fn var_sized<const N: usize>(name: &str) -> Result<Option<EnvSecret<N>>, Error> {
    let mut bytes = match std::env::var(name) {
        Ok(value) => value.into_bytes(),
        Err(VarError::NotPresent) => return Ok(None),
        Err(VarError::NotUnicode(value)) => {
            wipe(&mut value.into_encoded_bytes());
            return Err(Error::NotUnicode(name.to_owned()));
        }
    };
    let len = bytes.len();
    let secret = (len <= N).then(|| Secret::new_with(|page| page[..len].copy_from_slice(&bytes)));
    wipe(&mut bytes);
    let too_long = || Error::TooLong { name: name.to_owned(), len, max: N };
    let secret = secret.ok_or_else(too_long)?.map_err(Error::Memory)?;
    Ok(Some(EnvSecret { secret, len }))
}

fn wipe(bytes: &mut Vec<u8>) {
    let ptr = bytes.as_mut_ptr();
    for i in 0..bytes.capacity() {
        // SAFETY: `i` is inside the allocation, and spare capacity is writable.
        unsafe { ptr.add(i).write_volatile(0) };
    }
    atomic::compiler_fence(atomic::Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    #[test]
    fn wipe_zeroes_spare_capacity() {
        let mut bytes = b"sk-old-and-longer".to_vec();
        bytes.truncate(2);
        super::wipe(&mut bytes);
        // SAFETY: `wipe` initialized every byte up to capacity.
        let all = unsafe { std::slice::from_raw_parts(bytes.as_ptr(), bytes.capacity()) };
        assert!(all.len() >= 17 && all.iter().all(|&b| b == 0));
    }
}
