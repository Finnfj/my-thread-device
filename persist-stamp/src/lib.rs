//! The stamp the Platform stores with Persisted State: which Firmware wrote
//! it, in which format. At boot it decides whether the stored state is still
//! ours or whether the Device has to Factory Reset (ADR 0009).
//!
//! Pure logic, so that it can be tested on the host; the Platform reads and
//! writes the stamp and does the Factory Reset.

#![no_std]

/// The longest Firmware ID a [`Stamp`] holds, in bytes.
pub const MAX_FIRMWARE_ID_LEN: usize = 32;

/// The longest encoded [`Stamp`], in bytes.
pub const MAX_LEN: usize = 2 + MAX_FIRMWARE_ID_LEN;

/// Which Firmware wrote the Persisted State, and in which format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stamp<'a> {
    firmware_id: &'a str,
    format_version: u16,
}

impl<'a> Stamp<'a> {
    /// The stamp of `firmware_id` writing format `format_version`.
    ///
    /// # Panics
    ///
    /// If `firmware_id` is empty or longer than [`MAX_FIRMWARE_ID_LEN`]
    /// bytes. Used in a `const`, that is a build error.
    pub const fn new(firmware_id: &'a str, format_version: u16) -> Self {
        assert!(!firmware_id.is_empty(), "the Firmware ID is empty");
        assert!(
            firmware_id.len() <= MAX_FIRMWARE_ID_LEN,
            "the Firmware ID is longer than MAX_FIRMWARE_ID_LEN bytes",
        );
        Self {
            firmware_id,
            format_version,
        }
    }

    pub const fn firmware_id(&self) -> &'a str {
        self.firmware_id
    }

    pub const fn format_version(&self) -> u16 {
        self.format_version
    }

    /// The stamp as stored: the format version (little-endian `u16`), then
    /// the Firmware ID's UTF-8 bytes.
    ///
    /// The format version comes first so that it stays readable however the
    /// rest of the stored layout changes.
    pub fn encode<'b>(&self, out: &'b mut [u8; MAX_LEN]) -> &'b [u8] {
        let id = self.firmware_id.as_bytes();
        out[..2].copy_from_slice(&self.format_version.to_le_bytes());
        out[2..2 + id.len()].copy_from_slice(id);
        &out[..2 + id.len()]
    }

    /// A stored stamp, or `None` if `bytes` is not one [`Stamp::encode`]
    /// could have written.
    pub fn decode(bytes: &'a [u8]) -> Option<Self> {
        let (version, id) = bytes.split_first_chunk::<2>()?;
        let firmware_id = core::str::from_utf8(id).ok()?;
        if firmware_id.is_empty() || firmware_id.len() > MAX_FIRMWARE_ID_LEN {
            return None;
        }
        Some(Self::new(firmware_id, u16::from_le_bytes(*version)))
    }
}

/// What the stored stamp says about the Persisted State next to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Written by this Firmware in this format: keep it.
    Matches,
    /// No stamp: the partition is freshly erased, or the state was written
    /// before the Platform stored stamps. Keep it and store the stamp.
    Fresh,
    /// Written by another Firmware, in another format, or the stamp is
    /// unreadable: Factory Reset, then store the stamp.
    Mismatch,
}

/// Decide what to do with the Persisted State, given the stamp stored with
/// it (`None` if there is none) and the running Firmware's own stamp.
pub fn check(stored: Option<&[u8]>, current: &Stamp<'_>) -> Verdict {
    match stored {
        None => Verdict::Fresh,
        Some(bytes) if Stamp::decode(bytes).as_ref() == Some(current) => Verdict::Matches,
        Some(_) => Verdict::Mismatch,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CURRENT: Stamp<'static> = Stamp::new("on-off-light", 3);

    fn stored(stamp: Stamp<'_>, check_with: &Stamp<'_>) -> Verdict {
        let mut buf = [0; MAX_LEN];
        check(Some(stamp.encode(&mut buf)), check_with)
    }

    #[test]
    fn same_firmware_and_format_matches() {
        assert_eq!(stored(CURRENT, &CURRENT), Verdict::Matches);
    }

    #[test]
    fn no_stamp_is_fresh() {
        assert_eq!(check(None, &CURRENT), Verdict::Fresh);
    }

    #[test]
    fn other_firmware_id_is_a_mismatch() {
        assert_eq!(
            stored(Stamp::new("temperature-sensor", 3), &CURRENT),
            Verdict::Mismatch
        );
    }

    #[test]
    fn firmware_id_with_the_same_prefix_is_a_mismatch() {
        assert_eq!(
            stored(Stamp::new("on-off-light-2", 3), &CURRENT),
            Verdict::Mismatch
        );
    }

    #[test]
    fn older_format_version_is_a_mismatch() {
        assert_eq!(
            stored(Stamp::new("on-off-light", 2), &CURRENT),
            Verdict::Mismatch
        );
    }

    #[test]
    fn newer_format_version_is_a_mismatch() {
        assert_eq!(
            stored(Stamp::new("on-off-light", 4), &CURRENT),
            Verdict::Mismatch
        );
    }

    #[test]
    fn unreadable_stamp_is_a_mismatch() {
        // Any version, then a Firmware ID one byte too long.
        let too_long = [b'x'; 2 + MAX_FIRMWARE_ID_LEN + 1];
        for bytes in [
            &[][..],
            &[3][..],
            &[3, 0][..],
            &[3, 0, 0xff, 0xfe][..],
            &too_long[..],
        ] {
            assert_eq!(check(Some(bytes), &CURRENT), Verdict::Mismatch, "{bytes:?}");
        }
    }

    #[test]
    fn encode_then_decode_round_trips() {
        let longest = Stamp::new("abcdefghijklmnopqrstuvwxyz012345", u16::MAX);
        let mut buf = [0; MAX_LEN];
        assert_eq!(Stamp::decode(longest.encode(&mut buf)), Some(longest));
    }

    #[test]
    fn encoding_is_version_then_id() {
        let mut buf = [0; MAX_LEN];
        assert_eq!(
            Stamp::new("ab", 0x0102).encode(&mut buf),
            &[0x02, 0x01, b'a', b'b']
        );
    }

    #[test]
    #[should_panic(expected = "empty")]
    fn empty_firmware_id_is_rejected() {
        Stamp::new("", 1);
    }

    #[test]
    #[should_panic(expected = "longer")]
    fn too_long_firmware_id_is_rejected() {
        Stamp::new("abcdefghijklmnopqrstuvwxyz0123456", 1);
    }
}
