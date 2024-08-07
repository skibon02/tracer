use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Mutex;
use std::{mem, thread};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{JoinHandle, ThreadId};
use std::time::Duration;
use log::{info, warn};
use ringbuf::traits::{Consumer, Observer, Producer};
use serde::{Deserialize, Serialize};
use crate::id_mapping::{IdStore, IdStoreMap};

/// Preallocate 50MB for trace buffer
pub const GLOBAL_CAPACITY: usize = 500_000_000;

pub const CLEANUP_THRESHOLD: usize = (GLOBAL_CAPACITY as f64 * 0.9) as usize;
pub const CLEANUP_BOTTOM_THRESHOLD: usize = 350_000_000;
pub const FLUSH_THRESHOLD: usize = 5_000_000;

pub static GLOBAL_STORAGE: Mutex<Option<GlobalStorage>> = Mutex::new(None);
static FINALIZE_STARTED: AtomicBool = AtomicBool::new(false);

pub struct GlobalStorage {
    inner: ringbuf::LocalRb<ringbuf::storage::Heap<u8>>,
    skipped_msr_pages_headers: Vec<LocalPacketHeader>,
    sending_thread: Option<JoinHandle<()>>
}

impl Default for GlobalStorage {
    fn default() -> Self {
        let jh = thread::spawn(|| {
            info!("Global_storage: connecting to remote...");
            let mut con = TcpStream::connect("127.0.0.1:4302").unwrap();
            info!("Global_storage: Connected!");

            loop {
                thread::sleep(Duration::from_millis(100));

                let is_finalizing = FINALIZE_STARTED.load(Ordering::Relaxed);
                if is_finalizing {
                    info!("Finalizing detected!");
                }

                // this thing should be fast
                let (slices, failed_pages) = if let Some(global_storage) = GLOBAL_STORAGE.lock().unwrap().as_mut() {
                    let failed_pages = global_storage.take_failed_pages();
                    if let Some((slice1, slice2)) = global_storage.try_take_buf(is_finalizing) {
                        (Some((slice1, slice2)), failed_pages)
                    }
                    else {
                        (None, failed_pages)
                    }
                }
                else {
                    (None, Vec::new())
                };

                // handle buffers
                if let Some((slice1, slice2)) = slices {
                    info!("took two fresh slices! sizes: {}, {}", slice1.len(), slice2.len());
                    con.write_all(&[0x01]).unwrap();
                    let total_len = slice1.len() + slice2.len();
                    let total_len_bytes = total_len.to_be_bytes();
                    con.write_all(&total_len_bytes).unwrap();
                    con.write_all(&slice1).unwrap();
                    con.write_all(&slice2).unwrap();
                }

                // handle failed pages
                if failed_pages.len() > 0 {
                    info!("Took {} failed pages", failed_pages.len());
                    for header in failed_pages {
                        let header = bincode::serialize(&header).unwrap();
                        let header_len = header.len().to_be_bytes();
                        con.write_all(&[0x02]).unwrap();
                        con.write_all(&header_len).unwrap();
                        con.write_all(&header).unwrap();
                    }
                }

                if is_finalizing {
                    break;
                }
            }
            info!("Quit from global storage thread!");
        });


        Self {
            inner: ringbuf::LocalRb::new(GLOBAL_CAPACITY),
            skipped_msr_pages_headers: Vec::new(),
            sending_thread: Some(jh)
        }
    }

}

impl GlobalStorage {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn push_buf(&mut self, header: LocalPacketHeader, buf: &[u8]) {
        // info!("Got local packet: {:?}", header);
        let header = bincode::serialize(&header).unwrap();
        let header_len = header.len().to_be_bytes();

        self.inner.push_slice(&header_len);
        self.inner.push_slice(&header);
        self.inner.push_slice(&buf);

        if self.inner.occupied_len() > CLEANUP_THRESHOLD {
            self.dump_sizes();
            warn!("BUFFER FULL! clearing...");
            let mut header_len = [0u8; 8];
            while self.inner.occupied_len() > CLEANUP_BOTTOM_THRESHOLD {
                self.inner.read_exact(&mut header_len).unwrap();
                let header_len = usize::from_be_bytes(header_len);
                let mut header_bytes = vec![0u8; header_len];
                self.inner.read_exact(&mut header_bytes).unwrap();
                let header = bincode::deserialize::<LocalPacketHeader>(&header_bytes).unwrap();
                let buf_len = header.buf_length;
                self.inner.skip(buf_len);
                self.skipped_msr_pages_headers.push(header);
            }
            self.dump_sizes();
        }
    }

    pub fn take_failed_pages(&mut self) -> Vec<LocalPacketHeader> {
        mem::take(&mut self.skipped_msr_pages_headers)
    }

    pub fn try_take_buf(&mut self, take_everything: bool) -> Option<(Vec<u8>, Vec<u8>)> {
        let threshold = if take_everything {
            0
        } else {
            FLUSH_THRESHOLD
        };
        if self.inner.occupied_len() > threshold {
            info!("Flushing..");
            let slices = self.inner.as_slices();
            let slices = (slices.0.to_vec(), slices.1.to_vec());
            self.inner.clear();
            Some(slices)
        }
        else {
            None
        }
    }

    fn dump_sizes(&self)  {
        info!("\n\n\t*** STORAGE METRICS DUMP***");
        info!("Occupied len: {}", self.inner.occupied_len());
        info!("Skipped pages count: {}", self.skipped_msr_pages_headers.len());
        info!("");
    }

    fn take_jh(&mut self) -> Option<JoinHandle<()>> {
        self.sending_thread.take()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LocalPacketHeader {
    pub thread_name: String,
    pub thread_id: u64,

    pub initial_timestamp: u64,
    pub end_timestamp: u64,

    pub id_store: IdStoreMap,
    pub buf_length: usize,
}

pub fn finalize() {
    FINALIZE_STARTED.store(true, Ordering::Relaxed);
    let jh = if let Some(global_storage) = GLOBAL_STORAGE.lock().unwrap().as_mut() {
        global_storage.take_jh()
    } else {
        None
    };

    if let Some(jh) = jh {
        info!("JH JOIN! waiting for sender to finish");
        jh.join().unwrap();
    }

}