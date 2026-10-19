//! Opening plugin libraries with explicit load flags (ADR 0023 item 1, R-M4, NFR-3).
//!
//! - Unix: `dlopen(path, RTLD_NOW | RTLD_LOCAL)`: every symbol resolved at load (a broken library fails
//!   here, not in the middle of a tick), and nothing it exports can satisfy another library's symbols.
//! - Windows: `LoadLibraryExW(path, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS)`:
//!   dependencies are searched in the plugin's own directory and the default safe directories, never in
//!   the current directory or `PATH`. The host never calls the process-wide `SetDefaultDllDirectories`.
//!
//! The path is always absolute, never a bare name (which would trigger the loader's search); a Windows
//! verbatim prefix (`\\?\C:\..`, from `canonicalize`) is rewritten to the plain form, which these flags
//! accept. The entry symbol is read as a raw pointer and null-checked before it becomes a function
//! pointer.

#![expect(
    unsafe_code,
    reason = "opens plugin libraries and reads the entry symbol (ADR 0023 item 1)"
)]

use std::ffi::{OsString, c_void};
use std::path::{Component, Path, PathBuf, Prefix};

use umbra_plugin_api::UmbraPluginEntry;

#[cfg(not(any(unix, windows)))]
compile_error!("umbra-plugin-host loads plugins on Unix and Windows only (NFR-3)");

/// The NUL-terminated name of the one exported symbol (REQ-P-1).
const ENTRY_SYMBOL: &[u8] = b"umbra_plugin_entry\0";

/// An open plugin library. Dropping it closes the library; [`Library::leak`] keeps it mapped.
pub(crate) struct Library {
    #[cfg(unix)]
    inner: libloading::os::unix::Library,
    #[cfg(windows)]
    inner: libloading::os::windows::Library,
}

impl Library {
    /// Opens `path` (made absolute first) with the platform's explicit flags.
    pub(crate) fn open(path: &Path) -> Result<Self, String> {
        let path = load_path(path).map_err(|e| format!("cannot make the path absolute: {e}"))?;
        #[cfg(unix)]
        {
            use libloading::os::unix::{Library, RTLD_LOCAL, RTLD_NOW};
            // SAFETY: opening a library runs its initialisers. Plugins are trusted code (NFR-4,
            // REQ-P-11, documented in the plugin guide); the path is absolute, so no search takes place.
            let inner = unsafe { Library::open(Some(path.as_path()), RTLD_NOW | RTLD_LOCAL) };
            inner.map(|inner| Self { inner }).map_err(|e| e.to_string())
        }
        #[cfg(windows)]
        {
            use libloading::os::windows::{
                LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, Library,
            };
            // SAFETY: as for Unix: trusted code (NFR-4); an absolute, non-verbatim path with explicit
            // search flags, so dependencies resolve from the plugin's directory and the safe defaults.
            let inner = unsafe {
                Library::load_with_flags(
                    path.as_path(),
                    LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
                )
            };
            inner.map(|inner| Self { inner }).map_err(|e| e.to_string())
        }
    }

    /// The `umbra_plugin_entry` function, or `None` if the symbol is missing or null.
    pub(crate) fn entry(&self) -> Option<UmbraPluginEntry> {
        // SAFETY: the symbol is only looked up and read as an untyped address (`*mut c_void` is the
        // phantom type, never dereferenced here); the library stays open while `self` lives.
        let symbol = unsafe { self.inner.get::<*mut c_void>(ENTRY_SYMBOL) }.ok()?;
        let address = symbol.as_raw_ptr();
        if address.is_null() {
            return None;
        }
        // SAFETY: a non-null code address exported under the name `umbra_plugin_entry`, which REQ-P-1
        // gives the type `UmbraPluginEntry` (trusted plugin, NFR-4); data and function pointers have the
        // same size on the supported 64-bit targets (ADR 0019). The caller keeps the library open for as
        // long as it uses the pointer (`LoadedImage` drops it last).
        Some(unsafe { std::mem::transmute::<*mut c_void, UmbraPluginEntry>(address) })
    }

    /// Keeps the library mapped for the rest of the process (`HostOptions::keep_loaded`, ADR 0021).
    pub(crate) fn leak(self) {
        let _handle = self.inner.into_raw();
    }
}

/// `path` made absolute, without a Windows verbatim prefix.
fn load_path(path: &Path) -> std::io::Result<PathBuf> {
    let path = std::path::absolute(path)?;
    Ok(strip_verbatim(&path).unwrap_or(path))
}

/// `\\?\C:\a` -> `C:\a` and `\\?\UNC\server\share\a` -> `\\server\share\a`; `None` for any other path
/// (always `None` on Unix, which has no path prefixes).
fn strip_verbatim(path: &Path) -> Option<PathBuf> {
    let mut components = path.components();
    let Some(Component::Prefix(prefix)) = components.next() else {
        return None;
    };
    let mut plain = match prefix.kind() {
        Prefix::VerbatimDisk(letter) => PathBuf::from(format!(
            "{}:{}",
            char::from(letter),
            std::path::MAIN_SEPARATOR
        )),
        Prefix::VerbatimUNC(server, share) => {
            let mut root = OsString::from(r"\\");
            root.push(server);
            root.push(r"\");
            root.push(share);
            root.push(r"\");
            PathBuf::from(root)
        }
        _ => return None,
    };
    for component in components {
        if !matches!(component, Component::RootDir) {
            plain.push(component);
        }
    }
    Some(plain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_paths_are_absolute() {
        let p = load_path(Path::new("relative/libx.so")).unwrap_or_else(|e| panic!("{e}"));
        assert!(p.is_absolute(), "{}", p.display());
    }

    #[cfg(unix)]
    #[test]
    fn unix_paths_have_no_verbatim_prefix() {
        assert_eq!(strip_verbatim(Path::new("/a/libx.so")), None);
    }

    #[cfg(windows)]
    #[test]
    fn verbatim_prefixes_are_stripped() {
        assert_eq!(
            strip_verbatim(Path::new(r"\\?\C:\plugins\x.dll")),
            Some(PathBuf::from(r"C:\plugins\x.dll"))
        );
        assert_eq!(
            strip_verbatim(Path::new(r"\\?\UNC\srv\share\x.dll")),
            Some(PathBuf::from(r"\\srv\share\x.dll"))
        );
        assert_eq!(strip_verbatim(Path::new(r"C:\plugins\x.dll")), None);
    }

    #[test]
    fn a_file_that_is_not_a_library_does_not_open() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        assert!(Library::open(&path).is_err());
    }
}
