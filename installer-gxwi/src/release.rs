//! Which Peios this is: the release the medium carries, read from the
//! system's own statement of it.
//!
//! The installer runs from the medium, so the running system is what would be
//! installed. installerd does not say which release that is on its first
//! page, and the page names it before anything is asked, so it is read here.

use serde::Serialize;

const OS_RELEASE: &str = "/usr/lib/os-release";

/// What the page says this is. Anything the system does not state is empty,
/// and the page leaves it out.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Release {
    /// `VERSION_ID`, as `2026.8`.
    pub version: String,
    /// `VARIANT`, as `Experimental`.
    pub variant: String,
}

pub fn read() -> Release {
    std::fs::read_to_string(OS_RELEASE).map(|text| from(&text)).unwrap_or_default()
}

/// os-release is shell assignments, one to a line, quoted or not.
fn from(text: &str) -> Release {
    let value = |name: &str| {
        text.lines()
            .filter_map(|line| line.trim().split_once('='))
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.trim().trim_matches(['"', '\'']).to_string())
            .unwrap_or_default()
    };
    Release { version: value("VERSION_ID"), variant: value("VARIANT") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_release_is_read_from_the_systems_own_statement() {
        let text = "NAME=\"Peios\"\nID=peios\nVERSION_ID=\"2026.8\"\nVERSION=\"2026.8 (Experimental)\"\n\
                    VARIANT=\"Experimental\"\nVARIANT_ID=experimental\n";
        assert_eq!(from(text), Release { version: "2026.8".into(), variant: "Experimental".into() });
    }

    #[test]
    fn what_is_not_stated_is_empty() {
        assert_eq!(from("NAME=Peios\n# VERSION_ID=1\n"), Release::default());
        assert_eq!(from("VERSION_ID=2026.8\n").variant, "");
    }
}
