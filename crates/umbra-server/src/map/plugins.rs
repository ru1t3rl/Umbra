//! Loaded plugins: engine plugin info to wire plugins (WI-14, REQ-X-5, ADR 0026).
//!
//! The engine writes a plugin's `origin` in display form (ASM-194): `plugins[<i>]/<path>` for a
//! file in a plugin directory, a path relative to the scenario root otherwise. The wire splits it
//! into `root` and a path relative to that root, and never carries an absolute path.

use std::path::Path;

use umbra_api_types as api;
use umbra_engine::handle::PluginReport;
use umbra_engine::{CapabilityInfo, PluginInfo};

use super::components::wire_kind;
use super::status::plugin_fault;

/// `(root, origin relative to root)` of a displayed origin.
fn split_origin(shown: &str) -> (String, String) {
    if let Some(rest) = shown.strip_prefix("plugins[")
        && let Some((index, tail)) = rest.split_once(']')
        && !index.is_empty()
        && index.bytes().all(|b| b.is_ascii_digit())
        && (tail.is_empty() || tail.starts_with('/'))
    {
        let relative = tail.trim_start_matches('/');
        let relative = if relative.is_empty() { "." } else { relative };
        return (format!("plugins[{index}]"), relative.to_owned());
    }
    // Never absolute, even if the engine has no confinement to write it relative: keep the file name.
    if Path::new(shown).is_absolute() {
        let name = Path::new(shown)
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        return ("scenario".to_owned(), name);
    }
    ("scenario".to_owned(), shown.to_owned())
}

fn capability(info: &CapabilityInfo) -> Option<api::CapabilityDto> {
    Some(api::CapabilityDto {
        kind: wire_kind(info.kind)?,
        id: info.id.as_str().to_owned(),
        version: info.version.to_string(),
        faulted: info.faulted,
    })
}

/// The wire plugin. A capability of a kind the wire does not know is left out.
#[must_use]
pub fn plugin(info: &PluginInfo) -> api::PluginDto {
    let (root, origin) = split_origin(&info.origin);
    api::PluginDto {
        id: info.id.to_string(),
        name: info.name.clone(),
        version: info.version.to_string(),
        abi_major: info.abi.0,
        abi_minor: info.abi.1,
        origin,
        root,
        capabilities: info.capabilities.iter().filter_map(capability).collect(),
    }
}

/// The wire plugin report.
#[must_use]
pub fn plugin_report(report: &PluginReport) -> api::PluginReport {
    api::PluginReport {
        plugins: report.plugins.iter().map(plugin).collect(),
        fault: report.fault.as_ref().map(plugin_fault),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origins_split_into_root_and_relative_path() {
        let cases = [
            ("plugins[0]/libwander.so", ("plugins[0]", "libwander.so")),
            ("plugins[12]/sub/x.so", ("plugins[12]", "sub/x.so")),
            ("plugins[1]", ("plugins[1]", ".")),
            ("lib/p.so", ("scenario", "lib/p.so")),
            ("plugins[x]/p.so", ("scenario", "plugins[x]/p.so")),
            ("plugins[0]p.so", ("scenario", "plugins[0]p.so")),
            ("/srv/abs/p.so", ("scenario", "p.so")),
        ];
        for (shown, (root, origin)) in cases {
            assert_eq!(
                split_origin(shown),
                (root.to_owned(), origin.to_owned()),
                "{shown}"
            );
        }
    }
}
