//! Persisted State on flash: the `matter-kv` partition (ADR 0006, ADR 0008).
//!
//! The Matter stack stores Commissioning State and Network State (the Thread
//! dataset) in it as a key-value map, through rs-matter-embassy's
//! [`SeqMapKvBlobStore`] over sequential-storage, which spreads writes over the
//! partition's sectors for wear levelling.
//!
//! Every blob must fit both rs-matter's `KV_BUF_SIZE` (4096 by default) and,
//! with sequential-storage's item header, one 4 KiB flash sector.

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
pub(crate) fn open(flash: FLASH<'static>) -> Store {
    let mut flash = FlashStorage::new(flash);

    let mut table_buf = [0u8; PARTITION_TABLE_MAX_LEN];
    let table =
        read_partition_table(&mut flash, &mut table_buf).expect("cannot read the partition table");
    let partition = table
        .iter()
        .find(|p| p.label_as_str() == PARTITION_LABEL)
        .expect("no matter-kv partition: flash from the repo root so espflash reads espflash.toml");
    assert_eq!(
        partition.partition_type(),
        PartitionType::Data(DataPartitionSubType::Undefined),
        "matter-kv must be a data partition with subtype `undefined`",
    );

    let range = partition.offset()..partition.offset() + partition.len();
    info!(
        "Persisted State in partition {PARTITION_LABEL} at {:#x}..{:#x}",
        range.start, range.end
    );

    SeqMapKvBlobStore::new(BlockingAsync::new(flash), range)
}
