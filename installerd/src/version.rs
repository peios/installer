//! Ordering two package versions the way peipkg does.
//!
//! A port of peipkg's `internal/version` (PCDS §2.2: epoch, upstream,
//! revision; §2.2.7 for the upstream tokenisation and pre-release
//! rules), because the one question the upgrade page has to answer --
//! is what this medium carries newer than what the disk holds? -- is a
//! question about peipkg's ordering, and peipkg has no command that
//! answers it without also acting. Kept deliberately literal to the Go,
//! including its comments' reasoning, so that the two stay comparable
//! by reading.

use std::cmp::Ordering;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    /// Canonical decimal; empty when the string carried none.
    epoch: String,
    segments: Vec<Segment>,
    /// Canonical decimal, required.
    revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Segment {
    numeric: bool,
    text: String,
    /// At or after the first `~` or the first recognised pre-release
    /// token; from that point every segment is pre-release.
    pre_release: bool,
}

/// Parse `[epoch:]upstream-revision`.
pub fn parse(s: &str) -> Result<Version, String> {
    let (epoch, rest) = match s.split_once(':') {
        Some((e, r)) => (
            decimal(e).map_err(|e| format!("invalid epoch in {s:?}: {e}"))?,
            r,
        ),
        None => (String::new(), s),
    };
    let Some((upstream, group)) = rest.rsplit_once('-') else {
        return Err(format!("{s:?} has no -revision"));
    };
    let revision = decimal(group).map_err(|e| format!("invalid revision in {s:?}: {e}"))?;
    if revision == "0" {
        return Err(format!("revision in {s:?} must be a positive integer"));
    }
    if upstream.is_empty()
        || !upstream
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'+' | b'-' | b'~'))
    {
        return Err(format!("invalid upstream in {s:?}"));
    }
    Ok(Version {
        epoch,
        segments: tokenize(upstream),
        revision,
    })
}

/// The ordering peipkg would apply: by epoch, then upstream, then
/// revision.
pub fn compare(a: &Version, b: &Version) -> Ordering {
    compare_numeric(&a.epoch, &b.epoch)
        .then_with(|| compare_segments(&a.segments, &b.segments))
        .then_with(|| compare_numeric(&a.revision, &b.revision))
}

/// Whether `candidate` would be an upgrade over `installed`.
pub fn is_newer(candidate: &str, installed: &str) -> Result<bool, String> {
    Ok(compare(&parse(candidate)?, &parse(installed)?) == Ordering::Greater)
}

/// ASCII digits only, no leading zeros (zero is the single digit "0").
fn decimal(s: &str) -> Result<String, String> {
    if s.is_empty() || !s.bytes().all(|c| c.is_ascii_digit()) {
        return Err(format!("{s:?} is not a decimal integer"));
    }
    if s.len() > 1 && s.starts_with('0') {
        return Err(format!("{s:?} has a leading zero"));
    }
    Ok(s.to_string())
}

fn tokenize(upstream: &str) -> Vec<Segment> {
    let bytes = upstream.as_bytes();
    let mut segments = Vec::new();
    let mut pre_release = false;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'~' {
            pre_release = true;
            i += 1;
        } else if matches!(c, b'.' | b'+' | b'-') {
            i += 1;
        } else if c.is_ascii_digit() {
            let j = i + bytes[i..].iter().take_while(|c| c.is_ascii_digit()).count();
            segments.push(Segment {
                numeric: true,
                text: upstream[i..j].to_string(),
                pre_release,
            });
            i = j;
        } else {
            let j = i + bytes[i..]
                .iter()
                .take_while(|c| c.is_ascii_alphabetic())
                .count();
            let j = if j == i { i + 1 } else { j }; // a byte that is none of the above still advances
            let text = &upstream[i..j];
            if rank(text) < RANK_OTHER {
                pre_release = true;
            }
            segments.push(Segment {
                numeric: false,
                text: text.to_string(),
                pre_release,
            });
            i = j;
        }
    }
    segments
}

const RANK_OTHER: u8 = 5;

fn rank(s: &str) -> u8 {
    match s.to_ascii_lowercase().as_str() {
        "dev" => 0,
        "alpha" | "a" => 1,
        "beta" | "b" => 2,
        "pre" => 3,
        "rc" => 4,
        _ => RANK_OTHER,
    }
}

fn compare_segments(a: &[Segment], b: &[Segment]) -> Ordering {
    let common = a.len().min(b.len());
    for i in 0..common {
        let c = compare_segment(&a[i], &b[i]);
        if c != Ordering::Equal {
            return c;
        }
    }
    if a.len() == b.len() {
        return Ordering::Equal;
    }
    // One continues past the other; its next segment decides. A
    // pre-release tail is something the shorter version has already
    // passed, so it makes the shorter side greater; an ordinary tail
    // makes the shorter side less.
    let (longer, shorter_is_a) = if b.len() > a.len() {
        (b, true)
    } else {
        (a, false)
    };
    let shorter_vs_longer = if longer[common].pre_release {
        Ordering::Greater
    } else {
        Ordering::Less
    };
    if shorter_is_a {
        shorter_vs_longer
    } else {
        shorter_vs_longer.reverse()
    }
}

fn compare_segment(a: &Segment, b: &Segment) -> Ordering {
    if a.pre_release != b.pre_release {
        return if a.pre_release {
            Ordering::Less
        } else {
            Ordering::Greater
        };
    }
    match (a.numeric, b.numeric) {
        (true, true) => compare_numeric(&a.text, &b.text),
        (false, false) => {
            let (ra, rb) = (rank(&a.text), rank(&b.text));
            if ra != rb {
                ra.cmp(&rb)
            } else if ra == RANK_OTHER {
                a.text.as_bytes().cmp(b.text.as_bytes())
            } else {
                Ordering::Equal
            }
        }
        (true, false) => {
            if b.pre_release {
                Ordering::Greater
            } else {
                Ordering::Less
            }
        }
        (false, true) => {
            if a.pre_release {
                Ordering::Less
            } else {
                Ordering::Greater
            }
        }
    }
}

fn compare_numeric(a: &str, b: &str) -> Ordering {
    let a = a.trim_start_matches('0');
    let b = b.trim_start_matches('0');
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

#[cfg(test)]
mod tests {
    use super::{is_newer, parse};

    fn newer(a: &str, b: &str) -> bool {
        is_newer(a, b).unwrap()
    }

    #[test]
    fn revisions_and_releases_order_as_peipkg_orders_them() {
        assert!(newer("2026.8-7", "2026.8-6"));
        assert!(!newer("2026.8-6", "2026.8-7"));
        assert!(!newer("2026.8-7", "2026.8-7"));
        assert!(newer("2026.9-1", "2026.8-12"));
        assert!(newer("2026.8-10", "2026.8-9"));
        assert!(newer("2027.1-1", "2026.12-3"));
    }

    #[test]
    fn pre_releases_sort_below_the_release_and_epochs_above_everything() {
        assert!(newer("1.0-1", "1.0~rc1-1"));
        assert!(newer("1.0~rc1-1", "1.0~beta-1"));
        assert!(newer("1.0.1-1", "1.0-1"));
        assert!(newer("1:0.1-1", "2.0-1"));
        assert!(newer("1.0-1", "1.0~1-1"));
    }

    #[test]
    fn a_version_without_a_revision_is_not_a_package_version() {
        assert!(parse("2026.8").is_err());
        assert!(parse("2026.8-0").is_err());
        assert!(parse("2026.8-07").is_err());
        assert!(parse("").is_err());
    }
}
