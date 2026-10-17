//! Guitar Pro 7/8's `Content/PartConfiguration`: which notations each track shows. Big-endian: the
//! number of views (the multitrack view, then one per track); per view a multi-rest byte, the
//! number of tracks in it and one flag byte per track (1 standard notation, 2 tablature, 4 slash,
//! 8 numbered); then the view on screen (0 is the multitrack view).

/// More views or tracks than this is a damaged file.
const MAX_COUNT: u32 = 4096;

/// The notation flags of each track, from the multitrack view. `None` when the bytes are not a
/// part configuration.
pub fn read(bytes: &[u8]) -> Option<Vec<u8>> {
    let int = |at: usize| -> Option<u32> { Some(u32::from_be_bytes(bytes.get(at..at.checked_add(4)?)?.try_into().ok()?)) };
    let views = int(0)?;
    if !(1..=MAX_COUNT).contains(&views) {
        return None;
    }
    // The multitrack view: its multi-rest byte at 4, its track count at 5, its flags after that.
    let count = usize::try_from(int(5).filter(|c| *c <= MAX_COUNT)?).ok()?;
    Some(bytes.get(9..9usize.checked_add(count)?)?.to_vec())
}

/// A part configuration for tracks with these notation flags: the multitrack view, then each
/// track on its own, the first track's view on screen.
pub fn write(flags: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let len = |n: usize| u32::try_from(n).unwrap_or(u32::MAX).to_be_bytes();
    out.extend(len(flags.len() + 1));
    out.push(0);
    out.extend(len(flags.len()));
    out.extend(flags);
    for f in flags {
        out.push(0);
        out.extend(len(1));
        out.push(*f);
    }
    out.extend(len(usize::from(!flags.is_empty())));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_files_guitar_pro_writes() {
        // One guitar as standard and tablature, then as standard only, then two guitars.
        let both = [0, 0, 0, 2, 0, 0, 0, 0, 1, 3, 0, 0, 0, 0, 1, 3, 0, 0, 0, 1];
        assert_eq!(read(&both), Some(vec![3]));
        let standard = [0, 0, 0, 2, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 1, 1, 0, 0, 0, 1];
        assert_eq!(read(&standard), Some(vec![1]));
        let two = [0, 0, 0, 3, 0, 0, 0, 0, 2, 3, 3, 0, 0, 0, 0, 1, 3, 0, 0, 0, 0, 1, 3, 0, 0, 0, 2];
        assert_eq!(read(&two), Some(vec![3, 3]));
    }

    #[test]
    fn writes_what_it_reads() {
        for flags in [vec![], vec![3], vec![1, 2, 6]] {
            assert_eq!(read(&write(&flags)).unwrap_or_default(), flags);
        }
        // The layout Guitar Pro writes for one guitar shown as standard and tablature.
        assert_eq!(write(&[3]), [0, 0, 0, 2, 0, 0, 0, 0, 1, 3, 0, 0, 0, 0, 1, 3, 0, 0, 0, 1]);
    }

    #[test]
    fn damaged_bytes_are_none_not_a_panic() {
        assert_eq!(read(&[]), None);
        assert_eq!(read(&[0, 0, 0, 0]), None, "no views");
        assert_eq!(read(&[0xff, 0xff, 0xff, 0xff, 0]), None, "too many views");
        assert_eq!(read(&[0, 0, 0, 1, 0, 0xff, 0xff, 0xff, 0xff]), None, "too many tracks");
        assert_eq!(read(&[0, 0, 0, 1, 0, 0, 0, 0, 9, 3]), None, "fewer flags than it says");
    }
}
