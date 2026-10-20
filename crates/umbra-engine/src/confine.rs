//! Path confinement of served loads (D-b, ADR 0026 amendment, ASM-162, REQ-X-8, NFR-4).
//!
//! A [`Confinement`] keeps every path a load names inside the scenario root: the requested scenario
//! file, the template and graph files it reads, and its plugin directories (native code), which may
//! also lie in a configured plugin root. A request is normalised lexically first, so a path that
//! leaves the root is refused without the disk being consulted (no existence oracle); only then are
//! symbolic links resolved. Paths shown to a client are root-relative, never absolute.
//!
//! The check runs before the loader reads the files; a link swapped in between could still escape.
//! That race is accepted on a localhost server (ASM-162).

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use umbra_templates::ScenarioInputs;

/// The directories a served load may read from.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Confinement {
    /// The scenario root, canonical.
    pub root: PathBuf,
    /// Further directories that may hold plugins, canonical.
    pub plugin_roots: Vec<PathBuf>,
}

/// Why a path was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ConfineError {
    /// The path lies, or leads through a link, outside the allowed directories. `shown` is safe to
    /// send to a client.
    #[error("path not allowed: {shown}")]
    PathNotAllowed {
        /// The path in display form.
        shown: String,
    },
    /// The path is a directory.
    #[error("not a file: {shown}")]
    NotAFile {
        /// The path in display form.
        shown: String,
    },
}

impl Confinement {
    /// A confinement to `root` (and `plugin_roots` for plugin directories); the paths are
    /// canonicalised here.
    ///
    /// # Errors
    /// The `io` error of a directory that cannot be canonicalised (for example, it does not exist).
    pub fn new(root: &Path, plugin_roots: &[PathBuf]) -> io::Result<Self> {
        Ok(Self {
            root: fs::canonicalize(root)?,
            plugin_roots: plugin_roots
                .iter()
                .map(fs::canonicalize)
                .collect::<io::Result<_>>()?,
        })
    }

    /// Resolves a requested scenario path: relative paths are taken against the root, then the path
    /// is normalised lexically and must stay inside the root (otherwise `PathNotAllowed` without any
    /// disk access, whether or not the target exists), then links are resolved and the result must
    /// still be inside it. A path that does not exist yet is returned as it would resolve, so the
    /// loader can report it.
    ///
    /// # Errors
    /// [`ConfineError::PathNotAllowed`]; [`ConfineError::NotAFile`] for a directory.
    pub fn resolve_request(&self, requested: &Path) -> Result<PathBuf, ConfineError> {
        let joined = if requested.is_absolute() {
            requested.to_path_buf()
        } else {
            self.root.join(requested)
        };
        let lexical = normalise(&joined);
        if !lexical.starts_with(&self.root) {
            let shown = if requested.is_absolute() {
                "<absolute path outside the scenario root>".to_owned()
            } else {
                requested.display().to_string()
            };
            return Err(ConfineError::PathNotAllowed { shown });
        }
        let resolved = canonical_lenient(&lexical)
            .filter(|p| p.starts_with(&self.root))
            .ok_or_else(|| ConfineError::PathNotAllowed {
                shown: self.display(&lexical),
            })?;
        if resolved.is_dir() {
            return Err(ConfineError::NotAFile {
                shown: self.display(&resolved),
            });
        }
        Ok(resolved)
    }

    /// Checks every input of a load: the scenario file, each template directory and template or
    /// graph file must resolve inside the root, each plugin directory inside the root or a plugin
    /// root. Links are followed, so a link out of the allowed directories is refused.
    ///
    /// # Errors
    /// [`ConfineError::PathNotAllowed`] naming the first offending path, in display form.
    pub fn check_inputs(&self, inputs: &ScenarioInputs) -> Result<(), ConfineError> {
        let in_root = std::iter::once(&inputs.scenario)
            .chain(&inputs.template_dirs)
            .chain(&inputs.files);
        for path in in_root {
            self.require(path, &[&self.root])?;
        }
        let plugin_homes: Vec<&PathBuf> = std::iter::once(&self.root)
            .chain(&self.plugin_roots)
            .collect();
        for path in &inputs.plugin_dirs {
            self.require(path, &plugin_homes)?;
        }
        Ok(())
    }

    /// `text` with the root and the plugin roots written in display form (nothing for the root
    /// before a separator, `.` for the root alone, `plugins[<index>]` for a plugin root), so a
    /// message that quotes an absolute path does not leak it.
    ///
    /// A root matches only as whole path components: the character before it is not part of a
    /// path, and the one after it is a separator, the end of the text, or not part of a path
    /// (`/srv/scn` is replaced in `/srv/scn/a` and `"/srv/scn"`, not in `/srv/scn2/a`). The
    /// longest root wins; a root of `/` is never scrubbed.
    #[must_use]
    pub fn scrub(&self, text: &str) -> String {
        let mut roots: Vec<(String, String)> = self
            .plugin_roots
            .iter()
            .enumerate()
            .map(|(i, r)| (r.display().to_string(), format!("plugins[{i}]")))
            .collect();
        roots.push((self.root.display().to_string(), ".".to_owned()));
        roots.retain(|(root, _)| root.len() > 1 || !root.starts_with(is_separator));
        roots.sort_by_key(|(root, _)| std::cmp::Reverse(root.len()));
        let mut out = String::with_capacity(text.len());
        let mut at = 0;
        while at < text.len() {
            let rest = &text[at..];
            let before = text[..at].chars().next_back();
            let hit = roots.iter().find(|(root, _)| {
                rest.starts_with(root.as_str())
                    && !before.is_some_and(is_path_char)
                    && !rest[root.len()..].chars().next().is_some_and(is_path_char)
            });
            if let Some((root, shown)) = hit {
                at += root.len();
                match text[at..].chars().next().filter(|c| is_separator(*c)) {
                    Some(sep) if shown == "." => at += sep.len_utf8(),
                    _ => out.push_str(shown),
                }
                continue;
            }
            let Some(c) = rest.chars().next() else { break };
            out.push(c);
            at += c.len_utf8();
        }
        out
    }

    /// The display form of a path: relative to the root (`.` for the root itself), a path in a
    /// plugin root as `plugins[<index>]/<relative path>`, and anything else as its file name below
    /// `<outside the scenario root>`. Never absolute, and never a link's target.
    #[must_use]
    pub fn display(&self, path: &Path) -> String {
        let lexical = normalise(path);
        let anchored = lexical
            .parent()
            .zip(lexical.file_name())
            .and_then(|(parent, name)| Some(canonical_lenient(parent)?.join(name)));
        let candidates = [Some(lexical.clone()), anchored, canonical_lenient(&lexical)];
        for candidate in candidates.iter().flatten() {
            if let Some(rel) = candidate.strip_prefix(&self.root).ok().map(relative) {
                return rel;
            }
            for (index, root) in self.plugin_roots.iter().enumerate() {
                if let Ok(rel) = candidate.strip_prefix(root) {
                    return match relative(rel) {
                        r if r == "." => format!("plugins[{index}]"),
                        r => format!("plugins[{index}]/{r}"),
                    };
                }
            }
        }
        let name = lexical
            .file_name()
            .map_or_else(String::new, |n| format!("/{}", n.to_string_lossy()));
        format!("<outside the scenario root>{name}")
    }

    /// `path` must resolve (links followed) inside one of `homes`.
    fn require(&self, path: &Path, homes: &[&PathBuf]) -> Result<(), ConfineError> {
        match canonical_lenient(path) {
            Some(real) if homes.iter().any(|home| real.starts_with(home)) => Ok(()),
            _ => Err(ConfineError::PathNotAllowed {
                shown: self.display(path),
            }),
        }
    }
}

/// `/`-separated normal components of a relative path; `.` when there are none.
fn relative(path: &Path) -> String {
    let parts: Vec<String> = path
        .components()
        .filter_map(|c| match c {
            Component::Normal(n) => Some(n.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    if parts.is_empty() {
        ".".to_owned()
    } else {
        parts.join("/")
    }
}

/// Removes `.` and applies `..` textually. Touches no file.
fn normalise(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Resolves links component by component, like `canonicalize`, but a part of the path that does
/// not exist is kept as written (so a path that is missing below a link out of the root is still
/// seen to lead out). `None` for a link that cannot be resolved, and for a `..` after a missing part.
fn canonical_lenient(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    let mut missing = false;
    for component in path.components() {
        match component {
            Component::CurDir => {}
            // Past a missing part nothing says what `..` leads to on disk once a link is involved
            // later in the path, so it is not resolved.
            Component::ParentDir if missing => return None,
            Component::ParentDir => {
                out.pop();
            }
            Component::Prefix(_) | Component::RootDir => out.push(component.as_os_str()),
            Component::Normal(name) => {
                out.push(name);
                if !missing {
                    if fs::symlink_metadata(&out).is_ok() {
                        out = fs::canonicalize(&out).ok()?;
                    } else {
                        missing = true;
                    }
                }
            }
        }
    }
    Some(out)
}

fn is_separator(c: char) -> bool {
    c == '/' || c == std::path::MAIN_SEPARATOR
}

/// Whether `c` can continue a path component, so a root cannot end or start inside one.
fn is_path_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '~')
}

#[cfg(test)]
mod tests {
    use super::{Confinement, canonical_lenient};
    use std::path::{Path, PathBuf};

    fn confinement(root: &str, plugin_roots: &[&str]) -> Confinement {
        Confinement {
            root: PathBuf::from(root),
            plugin_roots: plugin_roots.iter().map(PathBuf::from).collect(),
        }
    }

    #[test]
    fn scrub_replaces_whole_components_only() {
        let c = confinement("/srv/scn", &[]);
        assert_eq!(
            c.scrub("cannot read /srv/scn/a/b.toml: gone"),
            "cannot read a/b.toml: gone"
        );
        assert_eq!(c.scrub("/srv/scn2/x stays"), "/srv/scn2/x stays");
        assert_eq!(
            c.scrub("/srv/scn-old/x /srv/scn.bak"),
            "/srv/scn-old/x /srv/scn.bak"
        );
        assert_eq!(
            c.scrub("/other/srv/scn/x"),
            "/other/srv/scn/x",
            "not at a component start"
        );
    }

    #[test]
    fn scrub_handles_the_root_alone_and_a_trailing_separator() {
        let c = confinement("/srv/scn", &[]);
        assert_eq!(c.scrub("in /srv/scn"), "in .");
        assert_eq!(c.scrub("in \"/srv/scn\": bad"), "in \".\": bad");
        assert_eq!(c.scrub("in /srv/scn/"), "in ");
        assert_eq!(c.scrub("/srv/scn/a and /srv/scn/b"), "a and b");
    }

    #[test]
    fn scrub_prefers_the_longest_of_nested_roots() {
        let c = confinement("/srv/scn", &["/srv/scn/plugins", "/srv/scn/plugins/extra"]);
        assert_eq!(
            c.scrub("/srv/scn/plugins/extra/lib.so"),
            "plugins[1]/lib.so"
        );
        assert_eq!(c.scrub("/srv/scn/plugins/lib.so"), "plugins[0]/lib.so");
        assert_eq!(c.scrub("/srv/scn/plugins"), "plugins[0]");
        assert_eq!(c.scrub("/srv/scn/other"), "other");
    }

    #[test]
    fn scrub_never_uses_a_root_of_slash() {
        let c = confinement("/", &["/plugins"]);
        assert_eq!(
            c.scrub("cannot read /etc/passwd"),
            "cannot read /etc/passwd"
        );
        assert_eq!(c.scrub("/plugins/x.so"), "plugins[0]/x.so");
    }

    #[test]
    fn a_missing_part_is_kept_but_a_parent_step_after_it_is_not_resolved() {
        let missing = Path::new("/umbra-no-such-dir/a/b");
        assert_eq!(canonical_lenient(missing), Some(missing.to_path_buf()));
        assert_eq!(
            canonical_lenient(Path::new("/umbra-no-such-dir/a/../b")),
            None
        );
    }
}
