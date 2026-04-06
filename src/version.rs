//! Arch Linux version comparison following libalpm/vercmp semantics.
//!
//! Version format: [epoch:]version[-release]
//! - epoch: Optional numeric prefix (default 0)
//! - version: Main version string
//! - release: Package release number (pkgrel)

use std::cmp::Ordering;

/// Compare two Arch Linux version strings.
/// Returns Ordering based on which version is newer.
///
/// # Examples
/// ```
/// use reap::version::vercmp;
/// use std::cmp::Ordering;
///
/// assert_eq!(vercmp("1.0", "2.0"), Ordering::Less);
/// assert_eq!(vercmp("1.0.0-1", "1.0.0-2"), Ordering::Less);
/// assert_eq!(vercmp("1:1.0", "2.0"), Ordering::Greater); // epoch wins
/// ```
pub fn vercmp(a: &str, b: &str) -> Ordering {
    let (epoch_a, ver_a, rel_a) = parse_version(a);
    let (epoch_b, ver_b, rel_b) = parse_version(b);

    // Compare epoch first (higher epoch always wins)
    match epoch_a.cmp(&epoch_b) {
        Ordering::Equal => {}
        ord => return ord,
    }

    // Compare version
    match compare_version_strings(ver_a, ver_b) {
        Ordering::Equal => {}
        ord => return ord,
    }

    // Compare release (pkgrel)
    compare_version_strings(rel_a, rel_b)
}

/// Parse version string into (epoch, version, release) components.
fn parse_version(ver: &str) -> (u32, &str, &str) {
    let (epoch, rest) = if let Some(idx) = ver.find(':') {
        let epoch_str = &ver[..idx];
        let epoch = epoch_str.parse::<u32>().unwrap_or(0);
        (epoch, &ver[idx + 1..])
    } else {
        (0, ver)
    };

    let (version, release) = if let Some(idx) = rest.rfind('-') {
        (&rest[..idx], &rest[idx + 1..])
    } else {
        (rest, "")
    };

    (epoch, version, release)
}

/// Compare two version strings segment by segment.
/// Handles mixed alpha/numeric segments correctly.
fn compare_version_strings(a: &str, b: &str) -> Ordering {
    let segs_a = split_version_segments(a);
    let segs_b = split_version_segments(b);

    let max_len = segs_a.len().max(segs_b.len());

    for i in 0..max_len {
        let seg_a = segs_a.get(i);
        let seg_b = segs_b.get(i);

        match (seg_a, seg_b) {
            (Some(a), Some(b)) => match compare_segments(a, b) {
                Ordering::Equal => continue,
                ord => return ord,
            },
            (Some(a), None) => {
                // a has more segments - if it's alpha, a is LESS (pre-release)
                // if it's numeric, a is GREATER
                let a_is_num = a.chars().all(|c| c.is_ascii_digit());
                return if a_is_num {
                    Ordering::Greater
                } else {
                    Ordering::Less // alpha suffix means pre-release
                };
            }
            (None, Some(b)) => {
                // b has more segments
                let b_is_num = b.chars().all(|c| c.is_ascii_digit());
                return if b_is_num {
                    Ordering::Less
                } else {
                    Ordering::Greater // b is pre-release, a is release
                };
            }
            (None, None) => break,
        }
    }

    Ordering::Equal
}

/// Split a version string into segments (alternating numeric and alpha).
fn split_version_segments(ver: &str) -> Vec<&str> {
    let mut segments = Vec::new();
    let mut start = 0;
    let mut in_digit = false;

    for (i, c) in ver.char_indices() {
        let is_digit = c.is_ascii_digit();
        let is_sep = c == '.' || c == '_' || c == '+';

        if is_sep {
            if start < i {
                segments.push(&ver[start..i]);
            }
            start = i + 1;
            in_digit = false;
        } else if i > start && is_digit != in_digit {
            segments.push(&ver[start..i]);
            start = i;
            in_digit = is_digit;
        } else if i == start {
            in_digit = is_digit;
        }
    }

    if start < ver.len() {
        segments.push(&ver[start..]);
    }

    segments
}

/// Compare two version segments.
/// Numeric segments are compared as integers.
/// Alpha segments are compared lexicographically.
/// Numeric segments are always greater than alpha segments.
fn compare_segments(a: &str, b: &str) -> Ordering {
    let a_is_num = a.chars().all(|c| c.is_ascii_digit());
    let b_is_num = b.chars().all(|c| c.is_ascii_digit());

    match (a_is_num, b_is_num) {
        (true, true) => {
            // Both numeric: compare as integers
            let num_a: u64 = a.parse().unwrap_or(0);
            let num_b: u64 = b.parse().unwrap_or(0);
            num_a.cmp(&num_b)
        }
        (true, false) => Ordering::Greater, // Numeric > alpha
        (false, true) => Ordering::Less,    // Alpha < numeric
        (false, false) => a.cmp(b),         // Both alpha: lexicographic
    }
}

/// Check if version `a` is older than version `b`.
pub fn is_older(a: &str, b: &str) -> bool {
    vercmp(a, b) == Ordering::Less
}

/// Check if version `a` is newer than version `b`.
#[allow(dead_code)]
pub fn is_newer(a: &str, b: &str) -> bool {
    vercmp(a, b) == Ordering::Greater
}

/// Check if two versions are equal.
#[allow(dead_code)]
pub fn is_equal(a: &str, b: &str) -> bool {
    vercmp(a, b) == Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_versions() {
        assert_eq!(vercmp("1.0", "2.0"), Ordering::Less);
        assert_eq!(vercmp("2.0", "1.0"), Ordering::Greater);
        assert_eq!(vercmp("1.0", "1.0"), Ordering::Equal);
    }

    #[test]
    fn test_dotted_versions() {
        assert_eq!(vercmp("1.0.0", "1.0.1"), Ordering::Less);
        assert_eq!(vercmp("1.0.1", "1.0.0"), Ordering::Greater);
        assert_eq!(vercmp("1.2.3", "1.2.3"), Ordering::Equal);
    }

    #[test]
    fn test_release_versions() {
        assert_eq!(vercmp("1.0.0-1", "1.0.0-2"), Ordering::Less);
        assert_eq!(vercmp("1.0.0-2", "1.0.0-1"), Ordering::Greater);
        assert_eq!(vercmp("1.0.0-1", "1.0.0-1"), Ordering::Equal);
    }

    #[test]
    fn test_epoch() {
        assert_eq!(vercmp("1:1.0", "2.0"), Ordering::Greater);
        assert_eq!(vercmp("2.0", "1:1.0"), Ordering::Less);
        assert_eq!(vercmp("2:1.0", "1:2.0"), Ordering::Greater);
    }

    #[test]
    fn test_alpha_versions() {
        assert_eq!(vercmp("1.0alpha", "1.0beta"), Ordering::Less);
        assert_eq!(vercmp("1.0beta", "1.0"), Ordering::Less); // numeric > alpha
        assert_eq!(vercmp("1.0rc1", "1.0rc2"), Ordering::Less);
    }

    #[test]
    fn test_mixed_lengths() {
        assert_eq!(vercmp("1.0", "1.0.0"), Ordering::Less);
        assert_eq!(vercmp("1.0.0", "1.0"), Ordering::Greater);
    }

    #[test]
    fn test_real_world_packages() {
        // Firefox versions
        assert_eq!(vercmp("125.0-1", "126.0-1"), Ordering::Less);
        // Git versions
        assert_eq!(vercmp("2.44.0-1", "2.44.0-2"), Ordering::Less);
        // Kernel versions
        assert_eq!(vercmp("6.8.1-1", "6.8.2-1"), Ordering::Less);
    }

    #[test]
    fn test_helper_functions() {
        assert!(is_older("1.0", "2.0"));
        assert!(is_newer("2.0", "1.0"));
        assert!(is_equal("1.0", "1.0"));
    }
}
