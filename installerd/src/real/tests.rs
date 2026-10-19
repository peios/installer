use super::{Real, edition_of, fsck_verdict, version_of};
use crate::executor::Progress;
use std::path::PathBuf;

#[test]
fn the_edition_is_read_the_way_upgrade_peios_reads_it() {
    let text = "NAME=\"Peios\"\nID=peios\nVERSION_ID=\"2026.8\"\nVARIANT_ID=experimental\n";
    assert_eq!(edition_of(text).unwrap(), "dev.peios.peios-experimental");
    assert!(edition_of("ID=debian\nVARIANT_ID=x\n").is_err());
    assert!(edition_of("ID=peios\n").is_err());
    assert_eq!(
            version_of(
                "name:         dev.peios.peios-experimental\nversion:      2026.8-7\narchitecture: noarch\n"
            )
            .as_deref(),
            Some("2026.8-7")
        );
    assert_eq!(version_of("name: x\n"), None);
}

/// e2fsck's exit status is a bitmask: 1 and 2 are the tool having
/// done its job, and a repair that reports them as failure is one
/// that fails precisely when the disk needed it.
#[test]
fn fsck_exit_status_is_read_as_the_bitmask_it_is() {
    assert_eq!(
        fsck_verdict(0).unwrap().unwrap(),
        "The filesystem is clean."
    );
    assert_eq!(
        fsck_verdict(1).unwrap().unwrap(),
        "Problems were found and fixed."
    );
    assert!(fsck_verdict(2).unwrap().unwrap().contains("Reboot before"));
    assert!(fsck_verdict(3).unwrap().unwrap().contains("Reboot before"));
    assert!(fsck_verdict(4).unwrap_err().contains("could not fix"));
    // Corrected some and left some: the uncorrected ones win.
    assert!(fsck_verdict(5).unwrap_err().contains("could not fix"));
    assert!(fsck_verdict(8).unwrap_err().contains("could not run"));
    assert!(fsck_verdict(16).unwrap_err().contains("exit 16"));
    assert!(fsck_verdict(32).unwrap_err().contains("cancelled"));
}

/// A target root with the queues peiso would have staged into it.
struct Target {
    dir: PathBuf,
}

impl Target {
    fn new(name: &str, queues: &[(&str, &[&str])]) -> Target {
        let dir = std::env::temp_dir().join(format!("peios-seedq-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        for (queue, seeds) in queues {
            let path = dir.join("root/lcl/policy").join(queue);
            std::fs::create_dir_all(&path).unwrap();
            for seed in *seeds {
                std::fs::write(path.join(format!("{seed}.reg")), b"{}").unwrap();
            }
        }
        std::fs::create_dir_all(dir.join("root/lcl/policy")).unwrap();
        Target { dir }
    }

    fn settle(&self) -> Result<(), String> {
        struct Quiet;
        impl Progress for Quiet {
            fn phase(&self, _: &str, _: u8) {}
            fn log(&self, _: String) {}
        }
        let real = Real {
            medium: PathBuf::from("/nonexistent"),
            work: self.dir.clone(),
            force: false,
        };
        real.settle_seed_queues(&Quiet)
    }

    fn queue(&self, name: &str) -> Option<Vec<String>> {
        let path = self.dir.join("root/lcl/policy").join(name);
        let entries = std::fs::read_dir(path).ok()?;
        let mut names: Vec<String> = entries
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        Some(names)
    }
}

impl Drop for Target {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

fn root_mnt_is(target: &Target) -> PathBuf {
    target.dir.join("root")
}

/// The whole reason the queues exist: what only the medium applies
/// goes, and what only a machine applies arrives.
#[test]
fn settling_deletes_the_live_queue_and_promotes_the_install_one() {
    let t = Target::new(
        "both",
        &[
            ("autoapply.d", &["login-console"][..]),
            ("autoapply.live.d", &["lpsd-first-account"][..]),
            ("autoapply.install.d", &["oobe-service"][..]),
        ],
    );
    assert!(root_mnt_is(&t).is_dir());

    t.settle().unwrap();

    assert_eq!(
        t.queue("autoapply.d").unwrap(),
        vec!["login-console.reg", "oobe-service.reg"],
    );
    // Not emptied — gone. An empty directory would say an image
    // might have staged something there, and this one cannot.
    assert!(
        t.queue("autoapply.live.d").is_none(),
        "the live queue must be removed"
    );
    assert!(
        t.queue("autoapply.install.d").is_none(),
        "the install queue must be removed"
    );
}

/// An edition that names no live or install seeds ships no such
/// directory. Absent is not a failure; a target that could not be
/// installed because it had nothing to delete would be absurd.
#[test]
fn settling_an_image_with_neither_queue_does_nothing() {
    let t = Target::new("neither", &[("autoapply.d", &["login-console"][..])]);

    t.settle().unwrap();

    assert_eq!(t.queue("autoapply.d").unwrap(), vec!["login-console.reg"]);
}

/// A first boot has to reach the install seeds even on an edition
/// that stages nothing into the ordinary queue.
#[test]
fn the_install_queue_is_promoted_even_with_no_base_queue() {
    let t = Target::new(
        "install-only",
        &[("autoapply.install.d", &["oobe-service"][..])],
    );

    t.settle().unwrap();

    assert_eq!(t.queue("autoapply.d").unwrap(), vec!["oobe-service.reg"]);
}

/// A target with no policy directory at all is not an error either:
/// the queues are the edition's to ship, and this runs before
/// anything depends on them existing.
#[test]
fn settling_a_target_with_no_policy_directory_succeeds() {
    let dir = std::env::temp_dir().join(format!("peios-seedq-bare-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(dir.join("root")).unwrap();
    struct Quiet;
    impl Progress for Quiet {
        fn phase(&self, _: &str, _: u8) {}
        fn log(&self, _: String) {}
    }
    let real = Real {
        medium: PathBuf::from("/nonexistent"),
        work: dir.clone(),
        force: false,
    };

    assert!(real.settle_seed_queues(&Quiet).is_ok());
    std::fs::remove_dir_all(&dir).ok();
}

/// The key arrives where gxwid looks for it, with the certificate and
/// its fingerprint; a medium with none leaves the target without one.
#[test]
fn gxwis_key_is_carried_into_the_target() {
    let dir = std::env::temp_dir().join(format!("peios-gxwi-key-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    let medium = dir.join("medium/gxwi");
    std::fs::create_dir_all(&medium).unwrap();
    std::fs::create_dir_all(dir.join("root")).unwrap();
    for (name, text) in [
        ("key.pem", "key"),
        ("certificate.pem", "cert"),
        ("certificate.sha256", "AB:CD\n"),
    ] {
        std::fs::write(medium.join(name), text).unwrap();
    }
    struct Quiet;
    impl Progress for Quiet {
        fn phase(&self, _: &str, _: u8) {}
        fn log(&self, _: String) {}
    }
    let real = Real {
        medium: PathBuf::from("/nonexistent"),
        work: dir.clone(),
        force: false,
    };

    // Peios's cp -a refuses to copy what it cannot carry a descriptor
    // for, which is everything on a host without KACS, where the
    // packaging gate runs. There the copy is not looked at; it is the
    // same cp -a the system itself is copied with.
    let probe = dir.join("probe");
    std::fs::write(&probe, "").unwrap();
    let carries = std::process::Command::new("cp")
        .args([
            "-a",
            probe.to_string_lossy().as_ref(),
            dir.join("probed").to_string_lossy().as_ref(),
        ])
        .status()
        .is_ok_and(|status| status.success());
    real.carry_machine_key(&Quiet, &medium);
    if carries {
        let carried = dir.join("root/var/state/gxwi");
        assert_eq!(
            std::fs::read_to_string(carried.join("key.pem")).unwrap(),
            "key"
        );
        assert_eq!(
            std::fs::read_to_string(carried.join("certificate.sha256")).unwrap(),
            "AB:CD\n"
        );
    }

    std::fs::remove_dir_all(dir.join("root/var")).ok();
    real.carry_machine_key(&Quiet, &dir.join("medium/none"));
    assert!(!dir.join("root/var/state/gxwi").exists());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn partition_suffix_is_probed_not_assumed() {
    let sd = |p: &str| matches!(p, "/dev/sda1" | "/dev/sda2");
    assert_eq!(
        Real::partitions_of("/dev/sda", sd).unwrap(),
        ("/dev/sda1".into(), "/dev/sda2".into())
    );
    let nvme = |p: &str| matches!(p, "/dev/nvme0n1p1" | "/dev/nvme0n1p2");
    assert_eq!(
        Real::partitions_of("/dev/nvme0n1", nvme).unwrap(),
        ("/dev/nvme0n1p1".into(), "/dev/nvme0n1p2".into())
    );
    // Only the first partition present is not enough.
    assert!(Real::partitions_of("/dev/sda", |p| p == "/dev/sda1").is_err());
}

#[test]
fn mounted_detection_matches_partitions_but_not_lookalikes() {
    let mounts = "/dev/sda2 / ext4 rw 0 0\n/dev/sdb1 /media/peios iso9660 ro 0 0\n";
    assert!(Real::mounted_in(mounts, "/dev/sda").is_some());
    assert!(Real::mounted_in(mounts, "/dev/sdb").is_some());
    // /dev/sd is not a disk here, and /dev/sdc is untouched.
    assert!(Real::mounted_in(mounts, "/dev/sdc").is_none());
    let nvme = "/dev/nvme0n1p2 / ext4 rw 0 0\n";
    assert!(Real::mounted_in(nvme, "/dev/nvme0n1").is_some());
    assert!(Real::mounted_in(nvme, "/dev/nvme1n1").is_none());
}

/// What a watched command is for: it is looked in on while it runs,
/// and once more at its end, and everything else about it is as
/// `run` has it -- what it said is logged, and how it ended decides.
#[test]
fn a_watched_command_is_looked_in_on_while_it_runs() {
    struct Heard(std::sync::Mutex<Vec<String>>);
    impl Progress for Heard {
        fn phase(&self, _: &str, _: u8) {}
        fn log(&self, line: String) {
            self.0.lock().unwrap().push(line);
        }
    }
    let real = Real::default();

    let heard = Heard(Default::default());
    let mut looks = 0;
    let script = "echo said; echo complained >&2; sleep 0.7";
    real.run_watched(&heard, "sh", &["-c", script], &mut || looks += 1)
        .unwrap();
    assert!(
        looks >= 3,
        "looked in {looks} times in 0.7s, every 0.2s and once at the end"
    );
    assert_eq!(
        *heard.0.lock().unwrap(),
        [
            format!("$ sh -c {script}"),
            "  said".into(),
            "  complained".into()
        ]
    );

    // One that is over at once is still looked in on, at its end.
    let mut looks = 0;
    let failed = real.run_watched(&heard, "sh", &["-c", "exit 3"], &mut || looks += 1);
    assert_eq!(failed, Err("sh failed (exit 3)".into()));
    assert!(looks >= 1);

    // More than a pipe holds, which would hang if it were not read
    // until the command had ended.
    let quiet = Heard(Default::default());
    real.run_watched(&quiet, "sh", &["-c", "yes | head -c 300000"], &mut || {})
        .unwrap();
    assert!(quiet.0.lock().unwrap().len() > 1000);
}
