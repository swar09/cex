use std::{
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use disruptor::{BusySpin, ProcessorSettings, build_multi_producer};
use engine::commands::{CommandDispatcher, CommandEnvelope, ExchangeCommand};
use engine::events::{EventConsumer, OrderBookEvent};
use engine::exchange::Exchange;
use memmap2::MmapMut;
use thiserror::Error;

pub const WAL_MAGIC: &[u8; 4] = b"CWAL";
pub const WAL_VERSION: u16 = 1;
pub const HEADER_SIZE: usize = 64;
pub const RECORD_HEADER_SIZE: usize = 28;
pub const TAG_EVENT: u8 = 0x00;
pub const TAG_SEGMENT_END: u8 = 0xFF;

#[derive(Error, Debug)]
pub enum WalError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] bincode::Error),
    #[error("Corrupt header in WAL file: {0}")]
    CorruptHeader(String),
    #[error("Record size {record_size} exceeds segment capacity {segment_size}")]
    RecordTooLarge { record_size: usize, segment_size: usize },
}

#[derive(Debug, Clone)]
pub struct WalConfig {
    pub wal_dir: PathBuf,
    pub segment_size: usize,
    pub flush_interval: Duration,
}

impl Default for WalConfig {
    fn default() -> Self {
        Self {
            wal_dir: PathBuf::from("data/wal"),
            segment_size: 64 * 1024 * 1024,
            flush_interval: Duration::from_millis(5),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentHeader {
    pub magic: [u8; 4],
    pub version: u16,
    pub reserved_flags: u16,
    pub segment_id: u64,
    pub created_at: u64,
    pub base_seq: u64,
}

impl SegmentHeader {
    pub fn new(segment_id: u64, base_seq: u64) -> Self {
        let created_at = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_micros() as u64;

        Self {
            magic: *WAL_MAGIC,
            version: WAL_VERSION,
            reserved_flags: 0,
            segment_id,
            created_at,
            base_seq,
        }
    }

    pub fn write_to(&self, dest: &mut [u8]) {
        dest[0..4].copy_from_slice(&self.magic);
        dest[4..6].copy_from_slice(&self.version.to_le_bytes());
        dest[6..8].copy_from_slice(&self.reserved_flags.to_le_bytes());
        dest[8..16].copy_from_slice(&self.segment_id.to_le_bytes());
        dest[16..24].copy_from_slice(&self.created_at.to_le_bytes());
        dest[24..32].copy_from_slice(&self.base_seq.to_le_bytes());
        dest[32..64].fill(0);
    }

    pub fn read_from(src: &[u8]) -> Result<Self, WalError> {
        if src.len() < HEADER_SIZE {
            return Err(WalError::CorruptHeader("Header slice too small".into()));
        }
        let mut magic = [0u8; 4];
        magic.copy_from_slice(&src[0..4]);
        if &magic != WAL_MAGIC {
            return Err(WalError::CorruptHeader(format!("Invalid magic bytes: {:?}", magic)));
        }
        let version = u16::from_le_bytes(src[4..6].try_into().unwrap());
        let reserved_flags = u16::from_le_bytes(src[6..8].try_into().unwrap());
        let segment_id = u64::from_le_bytes(src[8..16].try_into().unwrap());
        let created_at = u64::from_le_bytes(src[16..24].try_into().unwrap());
        let base_seq = u64::from_le_bytes(src[24..32].try_into().unwrap());

        Ok(Self {
            magic,
            version,
            reserved_flags,
            segment_id,
            created_at,
            base_seq,
        })
    }
}

pub struct WalWriter {
    config: WalConfig,
    active_segment_id: u64,
    #[allow(dead_code)]
    file: File,
    mmap: MmapMut,
    write_offset: usize,
    unflushed_bytes: usize,
    last_flush: Instant,
    scratch_buf: Vec<u8>,
}

impl WalWriter {
    pub fn new(config: WalConfig) -> Result<Self, WalError> {
        fs::create_dir_all(&config.wal_dir)?;

        let (active_segment_id, file, mmap, write_offset) = Self::init_or_open_latest(&config)?;

        Ok(Self {
            config,
            active_segment_id,
            file,
            mmap,
            write_offset,
            unflushed_bytes: 0,
            last_flush: Instant::now(),
            scratch_buf: Vec::with_capacity(1024),
        })
    }

    fn segment_path(wal_dir: &Path, segment_id: u64) -> PathBuf {
        wal_dir.join(format!("wal_{:016}.wal", segment_id))
    }

    fn init_or_open_latest(config: &WalConfig) -> Result<(u64, File, MmapMut, usize), WalError> {
        let mut highest_segment_id = 0u64;

        if let Ok(entries) = fs::read_dir(&config.wal_dir) {
            for entry in entries.flatten() {
                let file_name = entry.file_name();
                let name = file_name.to_string_lossy();
                if name.starts_with("wal_") && name.ends_with(".wal") {
                    let num_part = &name[4..name.len() - 4];
                    if let Ok(id) = num_part.parse::<u64>()
                        && id > highest_segment_id {
                            highest_segment_id = id;
                        }
                }
            }
        }

        if highest_segment_id == 0 {
            let segment_id = 1u64;
            let path = Self::segment_path(&config.wal_dir, segment_id);
            let file = OpenOptions::new().read(true).write(true).create_new(true).open(&path)?;

            file.set_len(config.segment_size as u64)?;
            let mut mmap = unsafe { MmapMut::map_mut(&file)? };

            let header = SegmentHeader::new(segment_id, 0);
            header.write_to(&mut mmap[..HEADER_SIZE]);
            mmap.flush_async()?;

            Ok((segment_id, file, mmap, HEADER_SIZE))
        } else {
            let path = Self::segment_path(&config.wal_dir, highest_segment_id);
            let file = OpenOptions::new().read(true).write(true).open(&path)?;

            let mmap = unsafe { MmapMut::map_mut(&file)? };
            let _ = SegmentHeader::read_from(&mmap[..HEADER_SIZE])?;

            let mut offset = HEADER_SIZE;
            while offset + RECORD_HEADER_SIZE <= config.segment_size {
                let total_frame_len = u32::from_le_bytes(mmap[offset..offset + 4].try_into().unwrap()) as usize;
                let tag = mmap[offset + 20];

                if total_frame_len == 0 || tag == TAG_SEGMENT_END {
                    break;
                }
                if total_frame_len < RECORD_HEADER_SIZE || offset + total_frame_len > config.segment_size {
                    break;
                }
                offset += total_frame_len;
            }

            if offset + RECORD_HEADER_SIZE > config.segment_size || mmap[offset + 20] == TAG_SEGMENT_END {
                let next_segment_id = highest_segment_id + 1;
                let next_path = Self::segment_path(&config.wal_dir, next_segment_id);
                let next_file = OpenOptions::new().read(true).write(true).create_new(true).open(&next_path)?;

                next_file.set_len(config.segment_size as u64)?;
                let mut next_mmap = unsafe { MmapMut::map_mut(&next_file)? };

                let header = SegmentHeader::new(next_segment_id, 0);
                header.write_to(&mut next_mmap[..HEADER_SIZE]);
                next_mmap.flush_async()?;

                Ok((next_segment_id, next_file, next_mmap, HEADER_SIZE))
            } else {
                Ok((highest_segment_id, file, mmap, offset))
            }
        }
    }

    pub fn append_record<T: serde::Serialize>(&mut self, item: &T, seq: u64) -> Result<(), WalError> {
        self.scratch_buf.clear();
        bincode::serialize_into(&mut self.scratch_buf, item)?;

        let payload_len = self.scratch_buf.len();
        let total_frame_len = RECORD_HEADER_SIZE + payload_len;

        if total_frame_len > self.config.segment_size - HEADER_SIZE {
            return Err(WalError::RecordTooLarge {
                record_size: total_frame_len,
                segment_size: self.config.segment_size,
            });
        }

        if self.write_offset + total_frame_len > self.config.segment_size {
            self.rotate(seq)?;
        }

        let timestamp_ns = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos() as u64;

        let mut hasher = crc32fast::Hasher::new();
        hasher.update(&self.scratch_buf);
        let crc = hasher.finalize();

        let dest = &mut self.mmap[self.write_offset..self.write_offset + total_frame_len];

        dest[0..4].copy_from_slice(&(total_frame_len as u32).to_le_bytes());
        dest[4..12].copy_from_slice(&seq.to_le_bytes());
        dest[12..20].copy_from_slice(&timestamp_ns.to_le_bytes());
        dest[20] = TAG_EVENT;
        dest[21..24].fill(0);
        dest[24..28].copy_from_slice(&crc.to_le_bytes());
        dest[28..total_frame_len].copy_from_slice(&self.scratch_buf);

        self.write_offset += total_frame_len;
        self.unflushed_bytes += total_frame_len;

        Ok(())
    }

    pub fn append(&mut self, event: &OrderBookEvent) -> Result<(), WalError> {
        self.append_record(event, event.sequence())
    }

    pub fn append_command(&mut self, cmd: &ExchangeCommand, seq: u64) -> Result<(), WalError> {
        self.append_record(cmd, seq)
    }

    pub fn rotate(&mut self, next_base_seq: u64) -> Result<(), WalError> {
        if self.write_offset + RECORD_HEADER_SIZE <= self.config.segment_size {
            let remaining = (self.config.segment_size - self.write_offset) as u32;
            self.mmap[self.write_offset..self.write_offset + 4].copy_from_slice(&remaining.to_le_bytes());
            self.mmap[self.write_offset + 20] = TAG_SEGMENT_END;
        }

        self.mmap.flush()?;

        self.active_segment_id += 1;
        let next_path = Self::segment_path(&self.config.wal_dir, self.active_segment_id);

        let file = OpenOptions::new().read(true).write(true).create_new(true).open(&next_path)?;

        file.set_len(self.config.segment_size as u64)?;
        let mut mmap = unsafe { MmapMut::map_mut(&file)? };

        let header = SegmentHeader::new(self.active_segment_id, next_base_seq);
        header.write_to(&mut mmap[..HEADER_SIZE]);

        self.file = file;
        self.mmap = mmap;
        self.write_offset = HEADER_SIZE;
        self.unflushed_bytes = HEADER_SIZE;
        self.last_flush = Instant::now();

        Ok(())
    }

    pub fn flush_periodic(&mut self, force: bool) -> io::Result<bool> {
        if self.unflushed_bytes == 0 {
            return Ok(false);
        }

        if force || self.last_flush.elapsed() >= self.config.flush_interval {
            self.mmap.flush_async()?;
            self.last_flush = Instant::now();
            self.unflushed_bytes = 0;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    #[inline]
    pub fn active_segment_id(&self) -> u64 {
        self.active_segment_id
    }

    #[inline]
    pub fn write_offset(&self) -> usize {
        self.write_offset
    }

    #[inline]
    pub fn unflushed_bytes(&self) -> usize {
        self.unflushed_bytes
    }
}

pub fn wal_logger(consumer: EventConsumer, config: WalConfig) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("wal-logger".to_string())
        .spawn(move || {
            let mut consumer = consumer;
            let mut writer = match WalWriter::new(config) {
                Ok(w) => w,
                Err(e) => {
                    eprintln!("[wal-logger] Failed to initialize WAL writer: {e}");
                    return;
                },
            };

            let mut idle_spins: u32 = 0;

            loop {
                match consumer.poll() {
                    Ok(events) => {
                        idle_spins = 0;
                        for event in &events {
                            if let Err(e) = writer.append(event) {
                                eprintln!("[wal-logger] Append error: {e}");
                            }
                        }
                        if let Err(e) = writer.flush_periodic(false) {
                            eprintln!("[wal-logger] Periodic flush error: {e}");
                        }
                    },
                    Err(disruptor::Polling::NoEvents) => {
                        // Engine is idle: force-flush any pending unflushed bytes immediately
                        if let Err(e) = writer.flush_periodic(true) {
                            eprintln!("[wal-logger] Idle flush error: {e}");
                        }

                        if idle_spins < 100 {
                            std::hint::spin_loop();
                        } else if idle_spins < 500 {
                            thread::yield_now();
                        } else {
                            thread::sleep(Duration::from_micros(50));
                        }
                        idle_spins = idle_spins.saturating_add(1);
                    },
                    Err(disruptor::Polling::Shutdown) => {
                        // Engine shutdown: force-flush and exit cleanly
                        let _ = writer.flush_periodic(true);
                        break;
                    },
                }
            }
        })
        .expect("failed to spawn wal-logger worker")
}

/// Builds a single gated ingress Disruptor pipeline
pub fn build_gated_ingress_pipeline(buffer_size: usize, wal_config: WalConfig, exchange: Exchange) -> CommandDispatcher {
    let producer = build_multi_producer(buffer_size, CommandEnvelope::empty, BusySpin)
        .thread_name("wal-logger")
        .handle_events_and_state_with(
            |writer: &mut WalWriter, envelope: &CommandEnvelope, sequence, end_of_batch| {
                if let Some(cmd) = &envelope.command {
                    let _ = writer.append_command(cmd, sequence as u64);
                    let _ = writer.flush_periodic(end_of_batch);
                }
            },
            move || WalWriter::new(wal_config).expect("failed to init WAL writer"),
        )
        .and_then()
        .thread_name("matching-engine")
        .handle_events_and_state_with(
            |engine: &mut Exchange, envelope: &CommandEnvelope, _sequence, _end_of_batch| {
                if let Some(cmd) = &envelope.command {
                    let _ = engine.handle_cmd(cmd.clone());
                }
            },
            move || exchange,
        )
        .build();

    CommandDispatcher::new(producer)
}

#[cfg(test)]
mod tests {
    use disruptor::{BusySpin, build_single_producer};
    use domain::{NewOrder, OrderPlacedEvent, OrderType, Side, Symbol};
    use engine::events::{EventDispatcher, EventEnvelope};
    use std::fs;

    use super::*;

    struct TempDirGuard {
        path: PathBuf,
    }

    impl TempDirGuard {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("cex_wal_test_{}_{}", name, std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn sample_order_placed(seq: u64, order_id: u64) -> OrderBookEvent {
        OrderBookEvent::OrderPlaced(
            seq,
            Symbol::BtcInr,
            OrderPlacedEvent {
                order_id,
                user_id: 101,
                side: Side::Buy,
                price: 50_000,
                quantity: 2,
                order_type: OrderType::GoodTillCancel,
            },
        )
    }

    #[test]
    fn test_segment_header_roundtrip() {
        let header = SegmentHeader::new(42, 1000);
        let mut buf = [0u8; HEADER_SIZE];
        header.write_to(&mut buf);

        let decoded = SegmentHeader::read_from(&buf).expect("failed to read segment header");
        assert_eq!(decoded.magic, *WAL_MAGIC);
        assert_eq!(decoded.version, WAL_VERSION);
        assert_eq!(decoded.segment_id, 42);
        assert_eq!(decoded.base_seq, 1000);
        assert_eq!(decoded.created_at, header.created_at);
    }

    #[test]
    fn test_mmap_wal_writer_append_and_frame_layout() {
        let temp = TempDirGuard::new("append_layout");
        let config = WalConfig {
            wal_dir: temp.path.clone(),
            segment_size: 4096,
            flush_interval: Duration::from_millis(10),
        };

        let mut writer = WalWriter::new(config).expect("failed to create writer");
        let event = sample_order_placed(1, 100);

        writer.append(&event).expect("append should succeed");
        assert_eq!(writer.active_segment_id(), 1);
        assert!(writer.write_offset() > HEADER_SIZE);
        assert_eq!(writer.unflushed_bytes(), writer.write_offset() - HEADER_SIZE);

        let segment_path = WalWriter::segment_path(&temp.path, 1);
        let file_bytes = fs::read(&segment_path).expect("should read WAL file");
        assert_eq!(file_bytes.len(), 4096);

        let header = SegmentHeader::read_from(&file_bytes[..HEADER_SIZE]).unwrap();
        assert_eq!(header.segment_id, 1);

        let frame_len = u32::from_le_bytes(file_bytes[64..68].try_into().unwrap()) as usize;
        let seq = u64::from_le_bytes(file_bytes[68..76].try_into().unwrap());
        let tag = file_bytes[84];
        let crc = u32::from_le_bytes(file_bytes[88..92].try_into().unwrap());

        assert_eq!(seq, 1);
        assert_eq!(tag, TAG_EVENT);
        assert!(frame_len > RECORD_HEADER_SIZE);

        let payload = &file_bytes[92..64 + frame_len];
        let mut hasher = crc32fast::Hasher::new();
        hasher.update(payload);
        assert_eq!(hasher.finalize(), crc);

        let decoded_event: OrderBookEvent = bincode::deserialize(payload).expect("should deserialize");
        assert_eq!(decoded_event, event);
    }

    #[test]
    fn test_mmap_wal_writer_segment_rotation() {
        let temp = TempDirGuard::new("rotation");
        let config = WalConfig {
            wal_dir: temp.path.clone(),
            segment_size: 180,
            flush_interval: Duration::from_millis(10),
        };

        let mut writer = WalWriter::new(config).expect("failed to create writer");
        assert_eq!(writer.active_segment_id(), 1);

        writer.append(&sample_order_placed(1, 101)).expect("first append ok");
        assert_eq!(writer.active_segment_id(), 1);

        writer.append(&sample_order_placed(2, 102)).expect("second append triggers rotate");
        assert_eq!(writer.active_segment_id(), 2);

        let seg1_path = WalWriter::segment_path(&temp.path, 1);
        let seg2_path = WalWriter::segment_path(&temp.path, 2);
        assert!(seg1_path.exists());
        assert!(seg2_path.exists());

        let seg1_bytes = fs::read(&seg1_path).unwrap();
        let end_tag_offset = 64 + u32::from_le_bytes(seg1_bytes[64..68].try_into().unwrap()) as usize;
        assert_eq!(seg1_bytes[end_tag_offset + 20], TAG_SEGMENT_END);
    }

    #[test]
    fn test_flush_periodic_behavior() {
        let temp = TempDirGuard::new("periodic_flush");
        let config = WalConfig {
            wal_dir: temp.path.clone(),
            segment_size: 4096,
            flush_interval: Duration::from_millis(50),
        };

        let mut writer = WalWriter::new(config).expect("create writer ok");

        assert_eq!(writer.flush_periodic(false).unwrap(), false);

        writer.append(&sample_order_placed(1, 10)).unwrap();
        assert!(writer.unflushed_bytes() > 0);

        assert_eq!(writer.flush_periodic(false).unwrap(), false);
        assert!(writer.unflushed_bytes() > 0);

        assert_eq!(writer.flush_periodic(true).unwrap(), true);
        assert_eq!(writer.unflushed_bytes(), 0);
    }

    #[test]
    fn test_wal_logger_integration_with_disruptor() {
        let temp = TempDirGuard::new("integration");
        let config = WalConfig {
            wal_dir: temp.path.clone(),
            segment_size: 8192,
            flush_interval: Duration::from_millis(5),
        };

        let event_factory = || EventEnvelope { event: None };
        let builder = build_single_producer(64, event_factory, BusySpin).with_multi_consumer();
        let (poller, builder) = builder.new_event_poller();
        let producer = builder.build();

        let mut dispatcher = EventDispatcher::new(producer);
        let consumer = EventConsumer::new(poller);

        let handle = wal_logger(consumer, config);

        for i in 1..=10 {
            dispatcher.publish(sample_order_placed(i, i * 100));
        }

        thread::sleep(Duration::from_millis(100));

        let seg1_path = WalWriter::segment_path(&temp.path, 1);
        assert!(seg1_path.exists());
        let seg_bytes = fs::read(&seg1_path).unwrap();

        let mut offset = HEADER_SIZE;
        let mut count = 0;
        while offset + RECORD_HEADER_SIZE <= 8192 {
            let frame_len = u32::from_le_bytes(seg_bytes[offset..offset + 4].try_into().unwrap()) as usize;
            if frame_len == 0 {
                break;
            }
            let seq = u64::from_le_bytes(seg_bytes[offset + 4..offset + 12].try_into().unwrap());
            assert_eq!(seq, count + 1);
            count += 1;
            offset += frame_len;
        }

        assert_eq!(count, 10);
        drop(handle);
    }

    #[test]
    fn test_gated_disruptor_pipeline_wal_and_engine() {
        let temp = TempDirGuard::new("gated_pipeline");
        let config = WalConfig {
            wal_dir: temp.path.clone(),
            segment_size: 16384,
            flush_interval: Duration::from_millis(5),
        };

        let event_factory = || EventEnvelope { event: None };
        let egress_builder = build_single_producer(1024, event_factory, BusySpin).with_multi_consumer();
        let (mut egress_poller, egress_builder) = egress_builder.new_event_poller();
        let egress_producer = egress_builder.build();

        let mut exchange = Exchange::new(egress_producer);
        exchange.add_new_orderbook(Symbol::BtcInr);

        let mut holdings_1 = risk::risk_engine::Holdings::default();
        let mut holdings_2 = risk::risk_engine::Holdings::default();
        holdings_1.credit_asset(Symbol::BtcInr.get_quantity_unit().asset_id(), 1_000_000);
        holdings_2.credit_asset(Symbol::BtcInr.get_quantity_unit().asset_id(), 1_000_000);
        exchange.get_risk_engine_mut().add_account(1, 1_000_000_000, holdings_1);
        exchange.get_risk_engine_mut().add_account(2, 1_000_000_000, holdings_2);

        let mut dispatcher = build_gated_ingress_pipeline(1024, config, exchange);

        let sell_order = NewOrder {
            order_id: 1,
            user_id: 2,
            asset_id: Symbol::BtcInr.get_quantity_unit().asset_id(),
            order_type: OrderType::GoodTillCancel,
            side: Side::Sell,
            price: Some(100),
            quantity: 1,
        };
        let buy_order = NewOrder {
            order_id: 2,
            user_id: 1,
            asset_id: Symbol::BtcInr.get_quantity_unit().asset_id(),
            order_type: OrderType::GoodTillCancel,
            side: Side::Buy,
            price: Some(100),
            quantity: 1,
        };

        dispatcher.publish(ExchangeCommand::AddNewOrder(Symbol::BtcInr, sell_order.clone()));
        dispatcher.publish(ExchangeCommand::AddNewOrder(Symbol::BtcInr, buy_order.clone()));

        thread::sleep(Duration::from_millis(150));

        let wal_file_path = WalWriter::segment_path(&temp.path, 1);
        assert!(wal_file_path.exists());
        let wal_bytes = fs::read(&wal_file_path).unwrap();

        let frame1_len = u32::from_le_bytes(wal_bytes[64..68].try_into().unwrap()) as usize;
        assert!(frame1_len > RECORD_HEADER_SIZE);
        let payload1 = &wal_bytes[92..64 + frame1_len];
        let cmd1: ExchangeCommand = bincode::deserialize(payload1).expect("deserialize cmd 1");
        assert_eq!(cmd1, ExchangeCommand::AddNewOrder(Symbol::BtcInr, sell_order));

        let offset2 = 64 + frame1_len;
        let frame2_len = u32::from_le_bytes(wal_bytes[offset2..offset2 + 4].try_into().unwrap()) as usize;
        assert!(frame2_len > RECORD_HEADER_SIZE);
        let payload2 = &wal_bytes[offset2 + 28..offset2 + frame2_len];
        let cmd2: ExchangeCommand = bincode::deserialize(payload2).expect("deserialize cmd 2");
        assert_eq!(cmd2, ExchangeCommand::AddNewOrder(Symbol::BtcInr, buy_order));

        let mut events = Vec::new();
        while let Ok(mut guard) = egress_poller.poll() {
            for item in &mut guard {
                if let Some(event) = &item.event {
                    events.push(event.clone());
                }
            }
        }

        assert_eq!(events.len(), 3);
        assert!(matches!(events[0], OrderBookEvent::OrderPlaced(1, Symbol::BtcInr, ..)));
        assert!(matches!(events[1], OrderBookEvent::OrderPlaced(2, Symbol::BtcInr, ..)));
        assert!(matches!(events[2], OrderBookEvent::TradeExecuted(3, Symbol::BtcInr, ..)));
    }
}
