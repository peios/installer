//! Convert a copied medium's package set into an installed system's set.
//!
//! peiso owns the temporary-package policy. This list is also a checkpoint:
//! keep it until all package operations succeed, including on older media
//! where we synthesize the historical two-package fallback.

use std::collections::BTreeSet;
use std::path::Path;

pub(crate) const REMOVALS_REL: &str = "lcl/policy/medium-only-packages";
pub(crate) const LIVE_BOOT_PACKAGE: &str = "dev.peios.live-boot";
pub(crate) const MEDIUM_REPO: &str = "peios-medium";
const HEADER: &str = "peios-medium-packages-v1";
const COMPLETE: &str = "peios-medium-packages-complete-v1\n";
const LEGACY: &str =
    "peios-medium-packages-v1\nroot dev.peios.live-boot\ninitramfs dev.peios.live-boot-irf\n";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Scope {
    Root,
    Initramfs,
}

fn parse(text: &str) -> Result<Vec<(Scope, String)>, String> {
    let mut lines = text.split('\n');
    if lines.next() != Some(HEADER) || !text.contains('\n') || text.contains('\r') {
        return Err("unsupported medium-only package list format".into());
    }
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    for (index, line) in lines.enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let words: Vec<_> = line.split_ascii_whitespace().collect();
        let invalid = || format!("invalid medium-only package entry at line {}", index + 2);
        if words.len() != 2 {
            return Err(invalid());
        }
        let scope = match words[0] {
            "root" => Scope::Root,
            "initramfs" => Scope::Initramfs,
            _ => return Err(invalid()),
        };
        let name = words[1];
        if !name.as_bytes()[0].is_ascii_alphanumeric()
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b))
        {
            return Err(invalid());
        }
        let entry = (scope, name.to_owned());
        if !seen.insert(entry.clone()) {
            return Err(format!(
                "duplicate medium-only package entry at line {}",
                index + 2
            ));
        }
        result.push(entry);
    }
    Ok(result)
}

/// The command seam is the real peipkg CLI, so tests can fail a transaction
/// without formatting a disk or changing the process-wide PATH.
pub(crate) fn finish_copy(
    root: &Path,
    mut command: impl FnMut(&[&str]) -> Result<String, String>,
) -> Result<(), String> {
    let marker = root.join(REMOVALS_REL);
    let (text, legacy) = match std::fs::read_to_string(&marker) {
        Ok(text) => (text, false),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (LEGACY.to_owned(), true),
        Err(e) => return Err(format!("reading {}: {e}", marker.display())),
    };
    let root_s = root.to_string_lossy();
    let irf_s = root.join("boot/initramfs").to_string_lossy().into_owned();
    if text != COMPLETE {
        let removals = parse(&text)?;
        if legacy {
            write_checkpoint(&marker, &text)?;
        }
        command(&["--root", &root_s, "repo", "add", MEDIUM_REPO])?;
        // Query both roots before removing anything. Unlike treating every
        // failed `info` as absent, this preserves database and command errors.
        let mut plans = Vec::new();
        for (scope, target) in [
            (Scope::Root, root_s.as_ref()),
            (Scope::Initramfs, irf_s.as_str()),
        ] {
            if !removals.iter().any(|(s, _)| *s == scope) {
                continue;
            }
            #[derive(serde::Deserialize)]
            struct Installed {
                name: String,
            }
            let listing = command(&["--root", target, "list", "--json"])?;
            let installed: Vec<Installed> = serde_json::from_str(&listing)
                .map_err(|e| format!("reading installed packages under {target}: {e}"))?;
            let names: BTreeSet<_> = installed.into_iter().map(|p| p.name).collect();
            let remove: Vec<_> = removals
                .iter()
                .filter(|(s, name)| *s == scope && names.contains(name))
                .map(|(_, name)| name.as_str())
                .collect();
            plans.push((target, remove));
        }
        for (target, remove) in plans {
            if remove.is_empty() {
                continue;
            }
            let mut args = vec!["--root", target, "uninstall", "--yes"];
            args.extend(remove);
            command(&args)?;
        }
        command(&[
            "--root",
            &root_s,
            "install",
            "dev.peios.disk-boot",
            "--yes",
            "--allow-stale",
        ])?;
        // Repository removal deletes its configuration before its database
        // entry. A cleanup retry must not need repo-add or install again.
        write_checkpoint(&marker, COMPLETE)?;
    }
    command(&["--root", &root_s, "repo", "remove", MEDIUM_REPO])?;
    std::fs::remove_file(&marker).map_err(|e| format!("removing {}: {e}", marker.display()))
}

fn write_checkpoint(marker: &Path, text: &str) -> Result<(), String> {
    let parent = marker.parent().expect("fixed path has a parent");
    std::fs::create_dir_all(parent).map_err(|e| format!("creating {}: {e}", parent.display()))?;
    let temporary = marker.with_extension("tmp");
    std::fs::write(&temporary, text)
        .map_err(|e| format!("writing {}: {e}", temporary.display()))?;
    std::fs::rename(&temporary, marker).map_err(|e| format!("staging {}: {e}", marker.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);
    const DWE: &str = "peios-medium-packages-v1\nroot dev.peios.live-boot\ninitramfs dev.peios.live-boot-irf\nroot dev.peios.dwed\nroot dev.example.live-only\n";

    struct Target {
        root: std::path::PathBuf,
        packages: [BTreeSet<String>; 2],
        calls: Vec<Vec<String>>,
        fail: Option<&'static str>,
        repo_config: bool,
    }

    impl Target {
        fn new(manifest: Option<&str>) -> Self {
            let root = std::env::temp_dir().join(format!(
                "pei-550-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(root.join("lcl/policy")).unwrap();
            if let Some(text) = manifest {
                std::fs::write(root.join(REMOVALS_REL), text).unwrap();
            }
            Self {
                root,
                packages: [
                    [
                        "dev.peios.live-boot",
                        "dev.peios.dwed",
                        "dev.example.live-only",
                        "dev.example.keep",
                    ]
                    .map(String::from)
                    .into(),
                    ["dev.peios.live-boot-irf", "dev.example.keep-irf"]
                        .map(String::from)
                        .into(),
                ],
                calls: Vec::new(),
                fail: None,
                repo_config: true,
            }
        }
        fn run(&mut self) -> Result<(), String> {
            let root = self.root.clone();
            finish_copy(&root, |args| {
                self.calls
                    .push(args.iter().map(|s| s.to_string()).collect());
                let scope = usize::from(args[1].ends_with("/boot/initramfs"));
                match args[2] {
                    "list" => {
                        if self.fail == Some("list") {
                            return Err("database unavailable".into());
                        }
                        Ok(serde_json::to_string(
                            &self.packages[scope]
                                .iter()
                                .map(|name| serde_json::json!({"name":name}))
                                .collect::<Vec<_>>(),
                        )
                        .unwrap())
                    }
                    "uninstall" => {
                        if scope == 1 && self.fail == Some("initramfs") {
                            return Err("initramfs removal failed".into());
                        }
                        for name in &args[4..] {
                            assert!(self.packages[scope].remove(*name));
                        }
                        Ok(String::new())
                    }
                    "install" => {
                        if self.fail == Some("install") {
                            return Err("disk-boot install failed".into());
                        }
                        self.packages[0].insert("dev.peios.disk-boot".into());
                        Ok(String::new())
                    }
                    "repo" => {
                        if args[3] == "add" && !self.repo_config {
                            return Err("repository configuration missing".into());
                        }
                        if args[3] == "remove" {
                            self.repo_config = false;
                            if self.fail == Some("repo-remove") {
                                return Err("repo removal failed".into());
                            }
                        }
                        Ok(String::new())
                    }
                    _ => panic!("unexpected command: {args:?}"),
                }
            })
        }
        fn checkpoint(&self) -> bool {
            self.root.join(REMOVALS_REL).exists()
        }
    }
    impl Drop for Target {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.root).unwrap();
        }
    }

    #[test]
    fn removes_declared_packages_from_their_roots_and_keeps_the_edition() {
        let mut t = Target::new(Some(DWE));
        t.run().unwrap();
        assert_eq!(
            t.packages[0],
            ["dev.example.keep".into(), "dev.peios.disk-boot".into()].into()
        );
        assert_eq!(t.packages[1], ["dev.example.keep-irf".into()].into());
        assert!(!t.checkpoint());
        let root_remove = t.calls.iter().find(|a| a[2] == "uninstall").unwrap();
        assert_eq!(
            &root_remove[4..],
            [
                "dev.peios.live-boot",
                "dev.peios.dwed",
                "dev.example.live-only"
            ]
        );
    }

    #[test]
    fn resumes_after_the_live_boot_package_was_already_removed() {
        let mut t = Target::new(Some(DWE));
        t.fail = Some("initramfs");
        assert!(t.run().unwrap_err().contains("initramfs"));
        assert!(t.checkpoint());
        assert!(!t.packages[0].contains(LIVE_BOOT_PACKAGE));
        assert!(t.repo_config);
        assert!(!t.calls.iter().any(|a| a[2] == "repo" && a[3] == "remove"));
        t.calls.clear();
        t.fail = None;
        t.run().unwrap();
        assert!(!t.checkpoint());
        assert!(
            t.calls
                .iter()
                .filter(|a| a[2] == "uninstall")
                .all(|a| a[1].ends_with("/boot/initramfs"))
        );
    }

    #[test]
    fn preserves_the_checkpoint_on_install_and_repository_cleanup_failure() {
        for failure in ["install", "repo-remove"] {
            let mut t = Target::new(Some(DWE));
            t.fail = Some(failure);
            assert!(t.run().is_err());
            assert!(t.checkpoint());
            let expected = if failure == "repo-remove" {
                COMPLETE
            } else {
                DWE
            };
            assert_eq!(
                std::fs::read_to_string(t.root.join(REMOVALS_REL)).unwrap(),
                expected
            );
            assert_eq!(t.repo_config, failure != "repo-remove");
            t.calls.clear();
            t.fail = None;
            t.run().unwrap();
            assert!(!t.checkpoint());
            if failure == "repo-remove" {
                assert_eq!(t.calls.len(), 1);
                assert_eq!(&t.calls[0][2..], ["repo", "remove", MEDIUM_REPO]);
            }
        }
    }

    #[test]
    fn completed_swap_only_retries_cleanup_even_without_repository_configuration() {
        let mut t = Target::new(Some(COMPLETE));
        t.repo_config = false;
        let packages = t.packages.clone();
        t.run().unwrap();
        assert_eq!(t.calls.len(), 1);
        assert_eq!(&t.calls[0][2..], ["repo", "remove", MEDIUM_REPO]);
        assert_eq!(t.packages, packages);
        assert!(!t.checkpoint());
    }

    #[test]
    fn database_failure_is_not_treated_as_an_absent_package() {
        let mut t = Target::new(Some(DWE));
        t.fail = Some("list");
        assert!(t.run().unwrap_err().contains("database unavailable"));
        assert!(t.checkpoint());
        assert!(
            !t.calls
                .iter()
                .any(|a| a[2] == "uninstall" || a[2] == "install")
        );
        assert!(t.repo_config);
        assert!(!t.calls.iter().any(|a| a[2] == "repo" && a[3] == "remove"));
    }

    #[test]
    fn older_media_get_a_resumable_two_package_fallback() {
        let mut t = Target::new(None);
        t.fail = Some("initramfs");
        assert!(t.run().is_err());
        assert_eq!(
            std::fs::read_to_string(t.root.join(REMOVALS_REL)).unwrap(),
            LEGACY
        );
        t.fail = None;
        t.run().unwrap();
        assert!(t.packages[0].contains("dev.peios.dwed"));
        assert!(t.packages[0].contains("dev.example.live-only"));
        assert!(!t.checkpoint());
    }

    #[test]
    fn malformed_metadata_fails_before_any_package_command() {
        for text in [
            "",
            "peios-medium-packages-v1",
            "peios-medium-packages-v2\n",
            "peios-medium-packages-complete-v1",
            "peios-medium-packages-complete-v1\n\n",
            "peios-medium-packages-complete-v1\nroot x\n",
            "peios-medium-packages-v1\r\n",
            "peios-medium-packages-v1\nelsewhere dev.example.x\n",
            "peios-medium-packages-v1\nroot --cascade\n",
            "peios-medium-packages-v1\nroot ../x\n",
            "peios-medium-packages-v1\nroot x extra\n",
            "peios-medium-packages-v1\nroot x\nroot x\n",
        ] {
            let mut t = Target::new(Some(text));
            assert!(t.run().is_err(), "{text:?}");
            assert!(t.calls.is_empty());
            assert!(t.checkpoint());
        }
    }
}
