//! How far a copy has got, by how much of it has been written.
//!
//! The copy is one `cp` for each top-level entry of the image, and the
//! phase used to move once per entry. The entries are nothing like the
//! same size: `/usr` is nearly all of a system and comes almost last, so
//! the phase ran most of the way up in a moment and then stood still for
//! as long as the copy really took.
//!
//! `cp` does not say how far along it is, and need not. What the copy
//! will come to can be worked out from the image before it starts, and
//! how much of that has landed is what the target filesystem says it
//! holds, asked while `cp` runs.
//!
//! The first figure is an estimate. It counts what a filesystem spends on
//! a tree -- whole blocks for a file's contents, a block for a directory
//! -- and not what it spends besides: its own bookkeeping, a block for a
//! descriptor too big for the inode, a directory grown past one block. So
//! it comes out a little under, and the phase is held short of finished
//! until the copy says it is.

use std::collections::HashSet;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

/// The longest a symbolic link's target can be and still be kept in the
/// inode, on ext4; a longer one takes a block.
const FAST_LINK: u64 = 60;

/// How much a filesystem with `block`-byte blocks will hold more once
/// everything under `tree` has been copied onto it.
///
/// A file linked under several names is counted once, as `cp -a` makes
/// it once. Nothing on another filesystem is counted, as `cp -x` copies
/// nothing from one. What cannot be read is left out, which is what makes
/// this safe to ask of any tree: a wrong figure is a bar that is a little
/// off, and the copy that follows is what reports a tree it cannot read.
pub fn room_for(tree: &Path, block: u64) -> u64 {
    let block = block.max(1);
    let Ok(top) = std::fs::symlink_metadata(tree) else {
        return 0;
    };
    let mut linked = HashSet::new();
    let mut total = 0u64;
    let mut pending = vec![tree.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            // An entry's own metadata: a link is measured, not followed.
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            let kind = meta.file_type();
            if kind.is_dir() {
                total += block;
                if meta.dev() == top.dev() {
                    pending.push(entry.path());
                }
            } else if kind.is_file() {
                if meta.nlink() > 1 && !linked.insert((meta.dev(), meta.ino())) {
                    continue;
                }
                total += meta.len().div_ceil(block) * block;
            } else if kind.is_symlink() && meta.len() >= FAST_LINK {
                total += block;
            }
        }
    }
    total
}

/// How far along a copy is, in hundredths, when `written` of the `total`
/// it should come to has landed.
///
/// Never the whole: the estimate runs under, so the target passes it
/// before the copy is over, and it is the copy ending that finishes the
/// phase.
pub fn percent(written: u64, total: u64) -> u8 {
    if total == 0 {
        return 0;
    }
    (u128::from(written) * 100 / u128::from(total)).min(99) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tree() -> std::path::PathBuf {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "installerd-copying-{}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_file_takes_whole_blocks_and_an_empty_one_takes_none() {
        let dir = tree();
        fs::write(dir.join("one"), vec![0u8; 1]).unwrap();
        fs::write(dir.join("exact"), vec![0u8; 4096]).unwrap();
        fs::write(dir.join("over"), vec![0u8; 4097]).unwrap();
        fs::write(dir.join("empty"), b"").unwrap();
        assert_eq!(room_for(&dir, 4096), 4096 + 4096 + 8192);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_directory_takes_a_block_and_what_is_in_it_is_counted() {
        let dir = tree();
        fs::create_dir_all(dir.join("a/b")).unwrap();
        fs::write(dir.join("a/b/file"), vec![0u8; 10]).unwrap();
        assert_eq!(room_for(&dir, 4096), 3 * 4096);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_file_with_two_names_is_counted_once() {
        let dir = tree();
        fs::write(dir.join("name"), vec![0u8; 5000]).unwrap();
        fs::hard_link(dir.join("name"), dir.join("other")).unwrap();
        assert_eq!(room_for(&dir, 4096), 8192);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_link_is_measured_and_never_followed() {
        let dir = tree();
        let elsewhere = tree();
        fs::write(elsewhere.join("big"), vec![0u8; 100_000]).unwrap();
        std::os::unix::fs::symlink(&elsewhere, dir.join("short")).unwrap();
        std::os::unix::fs::symlink("x".repeat(80), dir.join("long")).unwrap();
        // A short target lives in the inode; a long one takes a block.
        // Neither is the hundred thousand bytes the first one points at.
        let short = elsewhere.as_os_str().len() as u64;
        let expected = if short >= FAST_LINK { 8192 } else { 4096 };
        assert_eq!(room_for(&dir, 4096), expected);
        fs::remove_dir_all(&dir).unwrap();
        fs::remove_dir_all(&elsewhere).unwrap();
    }

    #[test]
    fn a_tree_that_is_not_there_comes_to_nothing() {
        assert_eq!(room_for(Path::new("/nonexistent/installerd"), 4096), 0);
    }

    #[test]
    fn the_phase_follows_what_has_been_written_and_stops_short_of_the_end() {
        assert_eq!(percent(0, 1000), 0);
        assert_eq!(percent(250, 1000), 25);
        assert_eq!(percent(999, 1000), 99);
        // The estimate runs under, so the target passes it.
        assert_eq!(percent(1000, 1000), 99);
        assert_eq!(percent(5000, 1000), 99);
        // Nothing to go by is no progress, not a division by nothing.
        assert_eq!(percent(10, 0), 0);
        // And sizes no real disk has do not wrap.
        assert_eq!(percent(u64::MAX, u64::MAX), 99);
    }
}
