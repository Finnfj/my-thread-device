//! Persisted State on flash: the `matter-kv` partition (ADR 0006, ADR 0008).
//!
//! The Matter stack stores Commissioning State and Network State (the Thread
//! dataset) in it as a key-value map, through rs-matter-embassy's
//! [`SeqMapKvBlobStore`] over sequential-storage, which spreads writes over the
//! partition's sectors for wear levelling.
//!
//! Every blob must fit both rs-matter's `KV_BUF_SIZE` (4096 by default) and,
//! with sequential-storage's item header, one 4 KiB flash sector.
//!
//! # Keys
//!
//! - `0x0000..0x1000`: rs-matter's own (it ends at `VENDOR_KEYS_START`).
//! - `0x1000..0x1100`: rs-matter-embassy, which stores the persisted subset of
//!   the OpenThread settings (the active dataset, network and parent info, the
//!   SRP key) as one blob at `0x1000` (checked in rs-matter-embassy @d9456b8).
//! - `0x1100..0x1200`: the Platform's own, [`PLATFORM_KEYS`].
//! - `0x1200..0x1300`: the Firmware's Application State,
//!   [`APPLICATION_KEYS`], for example the key an rs-matter On/Off cluster
//!   handler stores its `OnOff` and `StartUpOnOff` attributes under.
//!
//! # Factory Reset on a mismatch (ADR 0009)
//!
//! The Platform stamps the partition with the Firmware ID and
//! [`FORMAT_VERSION`]. When [`open`] finds another stamp, it Factory Resets
//! before the Matter stack loads anything: it erases the whole partition,
//! which clears Commissioning State, Network State, Application State, the
//! SRP key and the Platform's own keys at once.

use core::ops::Range;

use embassy_embedded_hal::adapter::BlockingAsync;
use embedded_storage::nor_flash::{ErrorType, MultiwriteNorFlash, NorFlash, ReadNorFlash};
use esp_bootloader_esp_idf::partitions::{
    DataPartitionSubType, PARTITION_TABLE_MAX_LEN, PartitionType, read_partition_table,
};
use esp_hal::peripherals::FLASH;
use esp_storage::{FlashStorage, FlashStorageError};
use log::{info, warn};
use persist_stamp::{Stamp, Verdict};
use rs_matter_embassy::matter::persist::{KV_BUF_SIZE, KvBlobStore, VENDOR_KEYS_START};
use rs_matter_embassy::persist::SeqMapKvBlobStore;

/// Label of the Persisted State partition in the repo's `partitions.csv`.
const PARTITION_LABEL: &str = "matter-kv";

/// The version of the Persisted State format.
///
/// Bump it whenever the stored layout changes: ours (the Platform's keys and,
/// once it moves in, Application State) or rs-matter's (what rs-matter and
/// rs-matter-embassy store under their keys, so check on every Lockstep Set
/// upgrade). Every Device that takes the update then Factory Resets and has
/// to be commissioned again (ADR 0009).
///
/// - 2: rs-matter 0.5 and rs-matter-embassy @d9456b8. Key `0x1000` holds the
///   OpenThread settings blob instead of the bare SRP key.
pub(crate) const FORMAT_VERSION: u16 = 2;

/// The keys reserved for the Platform's own records. See the module docs for
/// the whole key layout.
pub(crate) const PLATFORM_KEYS: Range<u16> = 0x1100..0x1200;

/// The keys a Firmware stores its Application State under. See the module
/// docs for the whole key layout.
pub const APPLICATION_KEYS: Range<u16> = PLATFORM_KEYS.end..PLATFORM_KEYS.end + 0x100;

// rs-matter-embassy keeps the first 0x100 vendor keys.
const _: () = assert!(PLATFORM_KEYS.start >= VENDOR_KEYS_START + 0x100);

/// The key of the stamp: which Firmware wrote the Persisted State, in which
/// format.
const STAMP_KEY: u16 = PLATFORM_KEYS.start;

/// The store holding all Persisted State.
pub(crate) type Store = StoreOn<'static>;

/// The store over a [`FlashStorage`] that lives for `'d`.
type StoreOn<'d> = SeqMapKvBlobStore<BlockingAsync<FlashStorage<'d>>>;

/// Open the store on the `matter-kv` partition for the Firmware
/// `firmware_id`, Factory Resetting first if another Firmware or format wrote
/// it.
///
/// Call this once per boot, as [`open_on`] explains.
///
/// # Panics
///
/// If the flashed partition table has no `matter-kv` data partition, which
/// means espflash did not read the repo's `espflash.toml`: it only reads it
/// from the directory it runs in, so flash from the repository root. Running
/// on without Persisted State would silently lose Commissioning on every
/// reboot, so this stops the Device instead.
///
/// Also if `firmware_id` is empty or longer than 32 bytes, or if the flash
/// fails during a Factory Reset or while storing the stamp.
pub(crate) fn open<'d>(flash: FLASH<'d>, firmware_id: &str) -> StoreOn<'d> {
    open_on(FlashStorage::new(flash), firmware_id).0
}

/// Open the store on the `matter-kv` partition of `flash`, as [`open`] does,
/// and say what the stamp check found.
///
/// Make one [`FlashStorage`] per boot and do everything through it. On the
/// Test Board, a second one, made after the first had read the flash, could
/// not read the partition table (PR #23); the cause is not confirmed. Panics
/// as described on [`open`].
fn open_on<'d>(mut flash: FlashStorage<'d>, firmware_id: &str) -> (StoreOn<'d>, Verdict) {
    let range = partition_range(&mut flash);
    info!(
        "Persisted State in partition {PARTITION_LABEL} at {:#x}..{:#x}",
        range.start, range.end
    );

    let current = Stamp::new(firmware_id, FORMAT_VERSION);
    let mut buf = [0u8; KV_BUF_SIZE];

    // Read the stamp through a borrowed flash, so the flash is still ours to
    // erase afterwards.
    let verdict = {
        let mut store =
            SeqMapKvBlobStore::new(BlockingAsync::new(Borrowed(&mut flash)), range.clone());
        match store.load(STAMP_KEY, &mut buf) {
            Ok(stored) => {
                let verdict = persist_stamp::check(stored, &current);
                log_verdict(verdict, stored, &current);
                verdict
            }
            // Unreadable: the Matter stack could not load from it either.
            Err(e) => {
                warn!("Cannot read the Persisted State stamp ({e:?}): Factory Reset.");
                Verdict::Mismatch
            }
        }
    };

    if verdict == Verdict::Mismatch {
        flash
            .erase(range.start, range.end)
            .expect("Factory Reset: erasing matter-kv failed");
        info!("Factory Reset: erased {PARTITION_LABEL}.");
    }

    let mut store = SeqMapKvBlobStore::new(BlockingAsync::new(flash), range);
    if verdict != Verdict::Matches {
        let mut stamp = [0u8; persist_stamp::MAX_LEN];
        store
            .store(STAMP_KEY, current.encode(&mut stamp), &mut buf)
            .expect("storing the Persisted State stamp failed");
    }
    (store, verdict)
}

/// Say on the serial log what the stamp check found.
fn log_verdict(verdict: Verdict, stored: Option<&[u8]>, current: &Stamp<'_>) {
    let (id, version) = (current.firmware_id(), current.format_version());
    match verdict {
        Verdict::Matches => info!("Persisted State is {id}'s, format {version}."),
        Verdict::Fresh => {
            info!("Persisted State has no stamp yet: stamping it as {id}'s, format {version}.")
        }
        Verdict::Mismatch => match stored.and_then(Stamp::decode) {
            Some(found) => warn!(
                "Persisted State is {}'s, format {}, not {id}'s, format {version}: Factory Reset.",
                found.firmware_id(),
                found.format_version()
            ),
            None => warn!("Persisted State has an unreadable stamp {stored:?}: Factory Reset."),
        },
    }
}

/// The flash address range of the `matter-kv` partition, from the flashed
/// partition table. Panics as described on [`open`].
fn partition_range(flash: &mut FlashStorage<'_>) -> Range<u32> {
    let mut table_buf = [0u8; PARTITION_TABLE_MAX_LEN];
    let table = match read_partition_table(flash, &mut table_buf) {
        Ok(table) => table,
        // The flash size is what esp-storage measured; it rejects every read
        // past it, so a wrong size shows up here as a storage error.
        Err(e) => panic!(
            "cannot read the partition table: {e:?} (flash size {} bytes)",
            flash.capacity()
        ),
    };
    let partition = table
        .iter()
        .find(|p| p.label_as_str() == PARTITION_LABEL)
        .expect("no matter-kv partition: flash from the repo root so espflash reads espflash.toml");
    assert_eq!(
        partition.partition_type(),
        PartitionType::Data(DataPartitionSubType::Undefined),
        "matter-kv must be a data partition with subtype `undefined`",
    );

    partition.offset()..partition.offset() + partition.len()
}

/// A borrowed [`FlashStorage`] that a [`SeqMapKvBlobStore`] can work on.
///
/// embedded-storage forwards `ReadNorFlash` and `NorFlash` through `&mut T`,
/// but not `MultiwriteNorFlash`, which the store needs (checked in
/// embedded-storage 0.3.1).
struct Borrowed<'a, 'd>(&'a mut FlashStorage<'d>);

impl ErrorType for Borrowed<'_, '_> {
    type Error = FlashStorageError;
}

impl<'d> ReadNorFlash for Borrowed<'_, 'd> {
    const READ_SIZE: usize = <FlashStorage<'d> as ReadNorFlash>::READ_SIZE;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        ReadNorFlash::read(self.0, offset, bytes)
    }

    fn capacity(&self) -> usize {
        ReadNorFlash::capacity(self.0)
    }
}

impl<'d> NorFlash for Borrowed<'_, 'd> {
    const WRITE_SIZE: usize = <FlashStorage<'d> as NorFlash>::WRITE_SIZE;
    const ERASE_SIZE: usize = <FlashStorage<'d> as NorFlash>::ERASE_SIZE;

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        NorFlash::write(self.0, offset, bytes)
    }

    fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        NorFlash::erase(self.0, from, to)
    }
}

impl MultiwriteNorFlash for Borrowed<'_, '_> {}

/// On-device tests (ADR 0011): they erase the whole `matter-kv` partition.
#[cfg(test)]
#[embedded_test::tests(default_timeout = 10)]
mod tests {
    use esp_hal::peripherals::Peripherals;
    use esp_storage::FlashStorage;
    use persist_stamp::{Stamp, Verdict};
    use rs_matter_embassy::matter::persist::{KV_BUF_SIZE, KvBlobStore};

    /// A key rs-matter does not use, so it can't be mistaken for Matter data.
    const KEY: u16 = 0xfe00;
    const VALUE: &[u8] = b"stored before the reset";
    /// The Firmware the tests boot as.
    const FIRMWARE_ID: &str = "platform-tests";
    /// Another Firmware, whose Persisted State must not survive a boot as
    /// [`FIRMWARE_ID`].
    const OTHER_FIRMWARE_ID: &str = "other-firmware";

    #[init]
    fn init() -> Peripherals {
        esp_hal::init(esp_hal::Config::default())
    }

    // Each test makes a single FlashStorage, as `open_on` asks.

    // A check across a reset takes two tests, because probe-rs resets the
    // chip before each test and a test can't span a reset of its own.
    // probe-rs runs them in the order they appear in the test binary's symbol
    // table, which is this file's order: run the whole suite, or both tests
    // of a pair, for example with `cargo xtask hil survives_reset`.

    /// Before the reset: erase all of `matter-kv`, as a full flash erase
    /// would, then store [`VALUE`].
    #[test]
    fn survives_reset_1_store(p: Peripherals) {
        let mut flash = FlashStorage::new(p.FLASH);
        let range = super::partition_range(&mut flash);
        flash
            .erase(range.start, range.end)
            .expect("erasing matter-kv failed");
        let (mut store, _) = super::open_on(flash, FIRMWARE_ID);
        let mut buf = [0u8; KV_BUF_SIZE];

        store.store(KEY, VALUE, &mut buf).expect("store failed");
        assert_eq!(
            store.load(KEY, &mut buf).expect("load failed"),
            Some(VALUE),
            "the value did not read back before the reset",
        );
    }

    /// After the reset probe-rs makes before this test: [`VALUE`] is still
    /// there. Then remove it, so a later run of this test alone fails.
    #[test]
    fn survives_reset_2_load(p: Peripherals) -> Result<(), &'static str> {
        let mut store = super::open(p.FLASH, FIRMWARE_ID);
        let mut buf = [0u8; KV_BUF_SIZE];
        let same = match store.load(KEY, &mut buf) {
            Ok(Some(value)) => value == VALUE,
            Ok(None) => {
                return Err(
                    "nothing stored under the test key: survives_reset_1_store must run first, in the same run",
                );
            }
            Err(_) => return Err("load failed"),
        };
        store.remove(KEY, &mut buf).expect("remove failed");
        if same {
            Ok(())
        } else {
            Err("a different value is stored under the test key")
        }
    }

    /// Before the reset: erase all of `matter-kv`, boot as another Firmware,
    /// which stamps it as that Firmware's, and store [`VALUE`] as that
    /// Firmware's state.
    #[test]
    fn wipes_other_firmware_1_store(p: Peripherals) {
        let mut flash = FlashStorage::new(p.FLASH);
        let range = super::partition_range(&mut flash);
        flash
            .erase(range.start, range.end)
            .expect("erasing matter-kv failed");
        let (mut store, verdict) = super::open_on(flash, OTHER_FIRMWARE_ID);
        assert_eq!(verdict, Verdict::Fresh, "an erased matter-kv is not fresh");
        let mut buf = [0u8; KV_BUF_SIZE];

        store.store(KEY, VALUE, &mut buf).expect("store failed");
    }

    /// After the reset: booting as [`FIRMWARE_ID`] finds the other
    /// Firmware's stamp, Factory Resets, and stamps `matter-kv` as its own.
    #[test]
    fn wipes_other_firmware_2_boot(p: Peripherals) -> Result<(), &'static str> {
        let (mut store, verdict) = super::open_on(FlashStorage::new(p.FLASH), FIRMWARE_ID);
        if verdict != Verdict::Mismatch {
            return Err(
                "matter-kv held no other Firmware's stamp: wipes_other_firmware_1_store must run first, in the same run",
            );
        }
        let mut buf = [0u8; KV_BUF_SIZE];

        if store
            .load(KEY, &mut buf)
            .map_err(|_| "load failed")?
            .is_some()
        {
            return Err("the other Firmware's value survived the Factory Reset");
        }
        let mut expected = [0u8; persist_stamp::MAX_LEN];
        let expected = Stamp::new(FIRMWARE_ID, super::FORMAT_VERSION).encode(&mut expected);
        match store.load(super::STAMP_KEY, &mut buf) {
            Ok(Some(stamp)) if stamp == expected => Ok(()),
            Ok(_) => Err("matter-kv is not stamped as this Firmware's after the Factory Reset"),
            Err(_) => Err("loading the stamp failed"),
        }
    }
}
