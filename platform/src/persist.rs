//! Persisted State on flash: the `matter-kv` partition (ADR 0006, ADR 0008).
//!
//! The Matter stack stores Commissioning State and Network State (the Thread
//! dataset) in it as a key-value map, through rs-matter-embassy's
//! [`SeqMapKvBlobStore`] over sequential-storage, which spreads writes over the
//! partition's sectors for wear levelling.
//!
//! Every blob must fit both rs-matter's `KV_BUF_SIZE` (4096 by default) and,
//! with sequential-storage's item header, one 4 KiB flash sector.

use core::ops::Range;

use embassy_embedded_hal::adapter::BlockingAsync;
use esp_bootloader_esp_idf::partitions::{
    DataPartitionSubType, PARTITION_TABLE_MAX_LEN, PartitionType, read_partition_table,
};
use esp_hal::peripherals::FLASH;
use esp_storage::FlashStorage;
use log::info;
use rs_matter_embassy::persist::SeqMapKvBlobStore;

/// Label of the Persisted State partition in the repo's `partitions.csv`.
const PARTITION_LABEL: &str = "matter-kv";

/// The store holding all Persisted State.
pub(crate) type Store = SeqMapKvBlobStore<BlockingAsync<FlashStorage<'static>>>;

/// Open the store on the `matter-kv` partition.
///
/// # Panics
///
/// If the flashed partition table has no `matter-kv` data partition, which
/// means espflash did not read the repo's `espflash.toml`: it only reads it
/// from the directory it runs in, so flash from the repository root. Running
/// on without Persisted State would silently lose Commissioning on every
/// reboot, so this stops the Device instead.
pub(crate) fn open<'d>(flash: FLASH<'d>) -> SeqMapKvBlobStore<BlockingAsync<FlashStorage<'d>>> {
    let mut flash = FlashStorage::new(flash);
    let range = partition_range(&mut flash);
    info!(
        "Persisted State in partition {PARTITION_LABEL} at {:#x}..{:#x}",
        range.start, range.end
    );
    SeqMapKvBlobStore::new(BlockingAsync::new(flash), range)
}

/// The flash address range of the `matter-kv` partition, from the flashed
/// partition table. Panics as described on [`open`].
fn partition_range(flash: &mut FlashStorage<'_>) -> Range<u32> {
    let mut table_buf = [0u8; PARTITION_TABLE_MAX_LEN];
    let table =
        read_partition_table(flash, &mut table_buf).expect("cannot read the partition table");
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

/// On-device tests (ADR 0011): they erase the whole `matter-kv` partition.
#[cfg(test)]
#[embedded_test::tests(default_timeout = 10)]
mod tests {
    use esp_hal::peripherals::Peripherals;
    use esp_storage::FlashStorage;
    use rs_matter_embassy::matter::persist::KvBlobStore;

    /// A key rs-matter does not use, so it can't be mistaken for Matter data.
    const KEY: u16 = 0xfe00;
    const VALUE: &[u8] = b"stored before the reset";
    /// rs-matter's `KV_BUF_SIZE`: the scratch buffer the store works in.
    const BUF_SIZE: usize = 4096;

    #[init]
    fn init() -> Peripherals {
        esp_hal::init(esp_hal::Config::default())
    }

    /// Erase all of `matter-kv`, as a full flash erase would.
    fn erase(p: &mut Peripherals) {
        let mut flash = FlashStorage::new(p.FLASH.reborrow());
        let range = super::partition_range(&mut flash);
        flash
            .erase(range.start, range.end)
            .expect("erasing matter-kv failed");
    }

    // "A value stored in matter-kv survives a reset" takes two tests, because
    // probe-rs resets the chip before each test and a test can't span a
    // reset of its own. probe-rs runs them in the order they appear in the
    // test binary's symbol table, which is this file's order: run the whole
    // suite, or both tests, with `cargo xtask hil survives_reset`.

    /// Before the reset: erase `matter-kv`, then store [`VALUE`].
    #[test]
    fn survives_reset_1_store(mut p: Peripherals) {
        erase(&mut p);
        let mut store = super::open(p.FLASH);
        let mut buf = [0u8; BUF_SIZE];

        store.store(KEY, VALUE, &mut buf).expect("store failed");
        assert_eq!(
            store.load(KEY, &mut buf).expect("load failed"),
            Some(VALUE),
            "the value did not read back before the reset",
        );
    }

    /// After the reset probe-rs makes before this test: [`VALUE`] is still
    /// there. Then erase `matter-kv` again, so no test value stays behind.
    #[test]
    fn survives_reset_2_load(mut p: Peripherals) -> Result<(), &'static str> {
        let mut buf = [0u8; BUF_SIZE];
        let outcome = match super::open(p.FLASH.reborrow()).load(KEY, &mut buf) {
            Ok(Some(value)) if value == VALUE => Ok(()),
            Ok(Some(_)) => Err("a different value is stored under the test key"),
            Ok(None) => Err(
                "nothing stored under the test key: survives_reset_1_store must run first, in the same run",
            ),
            Err(_) => Err("load failed"),
        };
        erase(&mut p);
        outcome
    }
}
