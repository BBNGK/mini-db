#![allow(dead_code)]
use crate::storage::disk::{DiskManager, PAGE_SIZE};
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard};

// Specifies the number of pages that can be in buffer at a time
pub const BUFF_POOL_SIZE: usize = 64;

#[derive(Debug, Default, Clone)]
struct FrameMetadata {
    page_id: Option<u32>,
    // Number of active readers / writers
    pin_count: u32,
    // Flag for whether data has been modified by access methods since it was selected from disk
    is_dirty: bool,
}

#[derive(Debug)]
struct BufferManagerInternals {
    // Holds instance of DiskManager
    disk_manager: DiskManager,
    // NOTE(ansh): probably poor cache locality. mark for review later.
    // Hashes page_id --> index in cache vector for faster reads
    page_table: HashMap<u32, usize>,
    // Holds a list of the metadata a Frame needs
    // Separated from Frame so guards could access / modify the data via Arc<Mutex<>>
    // Doubles as the cache.
    frame_metadata: [FrameMetadata; BUFF_POOL_SIZE],
    // Fields for CLOCK.
    cache_reference_bits: [bool; BUFF_POOL_SIZE],
    cache_cursor: usize,
}

impl BufferManagerInternals {
    // Helper fn for finding suitable Frame idx to be replaced. Implements CLOCK.
    // https://www.josehu.com/technical/2020/08/07/cache-eviction-algorithms.html
    pub fn find_replaceable_frame(&mut self) -> Option<usize> {
        // prevent replacement if all frames are pinned.
        if self.frame_metadata.iter().all(|f| f.pin_count > 0) {
            return None;
        }

        while self.cache_reference_bits[self.cache_cursor]
            || self.frame_metadata[self.cache_cursor].pin_count > 0
        {
            if self.frame_metadata[self.cache_cursor].pin_count > 0 {
                self.cache_cursor = (self.cache_cursor + 1) % BUFF_POOL_SIZE;
                continue;
            }

            self.cache_reference_bits[self.cache_cursor] = false;
            self.cache_cursor = (self.cache_cursor + 1) % BUFF_POOL_SIZE;
        }

        Some(self.cache_cursor)
    }
}

// An abstraction over the &[u8; PAGE_SIZE] format.
//
// Gets handed to caller when get_page is called, allowing for safe reads to the data by simply dereferencing,
// as well as handles cleanup so caller doesn't need to manually call unpin_page() properly.
#[derive(Debug)]
pub struct ReadPageGuard<'a> {
    inner: Arc<Mutex<BufferManagerInternals>>,
    frame_idx: usize,
    guard: RwLockReadGuard<'a, [u8; PAGE_SIZE]>,
}

impl<'a> Deref for ReadPageGuard<'a> {
    type Target = [u8; PAGE_SIZE];

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

// Handles unpinning page on Drop (instead of manually calling for unpinning)
//
// Doesn't need to check is_dirty, since a ReadPageGuard can never modify the data
impl<'a> Drop for ReadPageGuard<'a> {
    fn drop(&mut self) {
        let mut inner = self.inner.lock().unwrap();

        let frame_meta = &mut inner.frame_metadata[self.frame_idx];

        frame_meta.pin_count = frame_meta.pin_count.saturating_sub(1);
    }
}

// An abstraction over the &mut [u8; PAGE_SIZE] format.
//
// Gets handed to caller when get_page_mut is called, allowing for safe reads and writes to the data by simply dereferencing (mutably),
// as well as handles cleanup so caller doesn't need to manually call unpin_page() properly.
#[derive(Debug)]
pub struct WritePageGuard<'a> {
    inner: Arc<Mutex<BufferManagerInternals>>,
    frame_idx: usize,
    guard: RwLockWriteGuard<'a, [u8; PAGE_SIZE]>,
}

impl<'a> Deref for WritePageGuard<'a> {
    type Target = [u8; PAGE_SIZE];

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

impl<'a> DerefMut for WritePageGuard<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.guard
    }
}

// Handles unpinning page and flagging is_dirty as true, since it's assumed that
// a write has occurred, simply for using WritePageGuard
impl<'a> Drop for WritePageGuard<'a> {
    fn drop(&mut self) {
        let mut inner = self.inner.lock().unwrap();
        let frame_meta = &mut inner.frame_metadata[self.frame_idx];

        frame_meta.is_dirty = true;

        frame_meta.pin_count = frame_meta.pin_count.saturating_sub(1);
    }
}

pub struct BufferManager {
    // Holds the inner field data for BufferManager in a thread-safe + RC wrapper for guards
    internals: Arc<Mutex<BufferManagerInternals>>,
    // Holds BUFF_POOL_SIZE pages at a time in cache
    cache: [RwLock<[u8; PAGE_SIZE]>; BUFF_POOL_SIZE],
}

impl Drop for BufferManager {
    fn drop(&mut self) {
        let _ = self.flush_all();
    }
}

impl BufferManager {
    pub fn new(disk_manager: DiskManager) -> Self {
        Self {
            internals: Arc::new(Mutex::new(BufferManagerInternals {
                disk_manager,
                page_table: HashMap::new(),
                frame_metadata: core::array::from_fn(|_| FrameMetadata::default()),
                cache_cursor: 0,
                cache_reference_bits: [false; BUFF_POOL_SIZE],
            })),
            cache: core::array::from_fn(|_| RwLock::new([0u8; PAGE_SIZE])),
        }
    }

    pub fn allocate_page(&self) -> std::io::Result<u32> {
        self.internals.lock().unwrap().disk_manager.allocate_page()
    }

    // Handles obtaining page information from cache.
    //
    // If the page doesn't exist in cache, requests page from DiskManager and writes the page
    // to an available Frame in cache.
    //
    // Frames are replaced when they either have a pin_count of 0 or no page_id assigned, while also
    // making sure to flush dirty data back to disk before replacing.
    //
    pub fn get_page(&self, page_id: u32) -> ReadPageGuard<'_> {
        let mut inner = self.internals.lock().unwrap();

        match inner.page_table.get(&page_id).copied() {
            Some(idx) => {
                inner.frame_metadata[idx].pin_count += 1;

                drop(inner);

                ReadPageGuard {
                    frame_idx: idx,
                    guard: self.cache.get(idx).unwrap().read().unwrap(),
                    inner: Arc::clone(&self.internals),
                }
            }
            None => {
                let frame_idx = inner
                    .find_replaceable_frame()
                    .expect("get_page: all frames are pinned.");

                let old_frame_metadata = &inner.frame_metadata[frame_idx].clone();
                let mut frame = self.cache.get(frame_idx).unwrap().write().unwrap();

                // Flushes frame onto disk
                if old_frame_metadata.is_dirty {
                    inner
                        .disk_manager
                        .write_page(old_frame_metadata.page_id.unwrap(), &frame)
                        .expect("Failed to write page");
                    inner.frame_metadata[frame_idx].is_dirty = false;
                }

                // Removes old page from Hash
                if let Some(old_idx) = old_frame_metadata.page_id {
                    inner.page_table.remove(&old_idx);
                }

                // Inserts new page_idx into hash
                inner.page_table.insert(page_id, frame_idx);

                inner.frame_metadata[frame_idx].pin_count = 1;
                inner.frame_metadata[frame_idx].page_id = Some(page_id);
                inner.frame_metadata[frame_idx].is_dirty = false;

                // Writes new page data to buffer
                inner
                    .disk_manager
                    .read_page(page_id, &mut frame)
                    .expect("Failed to read page");

                drop(inner);

                let read_guard = RwLockWriteGuard::downgrade(frame);

                ReadPageGuard {
                    frame_idx,
                    guard: read_guard,
                    inner: Arc::clone(&self.internals),
                }
            }
        }
    }

    pub fn get_page_mut(&self, page_id: u32) -> WritePageGuard<'_> {
        let mut inner = self.internals.lock().unwrap();

        match inner.page_table.get(&page_id).copied() {
            Some(idx) => {
                inner.frame_metadata[idx].pin_count += 1;

                drop(inner);

                WritePageGuard {
                    inner: Arc::clone(&self.internals),
                    frame_idx: idx,
                    guard: self.cache.get(idx).unwrap().write().unwrap(),
                }
            }
            None => {
                let frame_idx = inner
                    .find_replaceable_frame()
                    .expect("get_page: all frames are pinned.");

                let old_frame_metadata = &inner.frame_metadata[frame_idx].clone();
                let mut frame = self.cache.get(frame_idx).unwrap().write().unwrap();

                // Flushes frame onto disk
                if old_frame_metadata.is_dirty {
                    // NOTE(ansh): ignores errors
                    _ = inner
                        .disk_manager
                        .write_page(old_frame_metadata.page_id.unwrap(), &frame);
                    inner.frame_metadata[frame_idx].is_dirty = false;
                }

                // Removes old page from Hash
                if let Some(old_idx) = old_frame_metadata.page_id {
                    inner.page_table.remove(&old_idx);
                }

                // Inserts new page_idx into hash
                inner.page_table.insert(page_id, frame_idx);

                inner.frame_metadata[frame_idx].pin_count = 1;
                inner.frame_metadata[frame_idx].page_id = Some(page_id);
                inner.frame_metadata[frame_idx].is_dirty = false;

                // Writes new page data to buffer
                // NOTE(ansh): ignores errors
                _ = inner.disk_manager.read_page(page_id, &mut frame);

                drop(inner);

                WritePageGuard {
                    frame_idx,
                    guard: frame,
                    inner: Arc::clone(&self.internals),
                }
            }
        }
    }

    // Flushes frames individually, to not cause deadlocks by locking internals then
    // attempting to free the entire frame pool at once.
    fn flush_frame(&self, frame_idx: usize) -> Result<(), std::io::Error> {
        let mut inner = self.internals.lock().unwrap();

        let page = self.cache[frame_idx].read().unwrap();

        if inner.frame_metadata[frame_idx].is_dirty {
            let page_id = inner.frame_metadata[frame_idx]
                .page_id
                .expect("Page is dirty w/o page_id?");
            inner.disk_manager.write_page(page_id, &page)?;
            inner.frame_metadata[frame_idx].is_dirty = false;
        }

        Ok(())
    }

    // Call to flush all dirty frames into disk.
    //
    // Useful for when BufferManager is Dropped (potential crash?), as well as for
    // "checkpointing" (future WAL / background thread for "cleanup"?).
    pub fn flush_all(&self) -> Result<(), std::io::Error> {
        for idx in 0..self.cache.len() {
            self.flush_frame(idx)?;
        }

        self.internals.lock().unwrap().disk_manager.sync()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::OpenOptions;
    use std::path::PathBuf;
    use std::time::SystemTime;

    struct TmpFile(std::path::PathBuf);

    impl TmpFile {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "{name}_{}.db",
                SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .expect("test_read_write (src/storage/disk.rs): shouldn't error")
                    .as_secs()
            ));
            Self(path)
        }
    }

    // Cleans up file from testing
    impl Drop for TmpFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    fn test_cache_eviction() -> std::io::Result<()> {
        let temp_file = TmpFile::new("cache_eviction");
        let temp_file = OpenOptions::new()
            .read(true)
            .write(true)
            .truncate(true)
            .create(true)
            .open(&temp_file.0)?;
        let disk_mgr = DiskManager::new(temp_file, 32);
        let mut buf_mgr = BufferManager::new(disk_mgr);

        // TODO(ansh): fix this with restructure

        // test: check eviction of first page in default config.

        // let replaceable_cache_idx = buf_mgr.find_replaceable_frame();
        // assert_eq!(replaceable_cache_idx, Some(0));

        // // test: replace with set configuration and test for first zero ref bit.

        // const TEST_FRAMES_COUNT: usize = 4;
        // for idx in 0..TEST_FRAMES_COUNT {
        //     // [
        //     //  (Frame, 1),
        //     //  (Frame, 1),
        //     //  (Frame, 0),
        //     //  (Frame, 1)
        //     //            ]
        //     buf_mgr.cache[idx] = (Frame::default(), idx != 2)
        // }
        // let replaceable_cache_idx = buf_mgr.find_replaceable_frame();
        // assert_eq!(replaceable_cache_idx, 2);

        // // test: all ref bits = 1.

        // for idx in 0..BUFF_POOL_SIZE {
        //     // [
        //     //  (false),
        //     //  (false),
        //     //  (false),
        //     //  (false),
        //     //       .
        //     //       .
        //     //       .
        //     //  (Frame, 1)
        //     //            ]
        //     buf_mgr.cache[idx] = true
        // }
        // buf_mgr.cache_mra_cursor = 1;
        // let replaceable_cache_idx = buf_mgr.find_replaceable_frame();
        // assert_eq!(replaceable_cache_idx, 1);

        Ok(())
    }

    fn new_buffer_pool(file_path: &PathBuf) -> BufferManager {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(file_path)
            .expect("Couldn't open test file!");

        let dm = DiskManager::new(file, 0);
        BufferManager::new(dm)
    }

    #[test]
    fn testing_cache_hit() {
        let test_file = TmpFile::new("test_cache_hit");
        let bm = new_buffer_pool(&test_file.0);

        let page_id = bm.allocate_page().unwrap();

        let mut data = bm.get_page_mut(page_id);
        data[0..11].copy_from_slice(b"HELLO THERE");

        drop(data);

        let reread_data = bm.get_page(page_id);
        assert_eq!(&reread_data[0..11], b"HELLO THERE");
    }

    #[test]
    fn test_flush_writes_to_disk() {
        let test_file = TmpFile::new("test_flush_writes_to_disk");

        // Writes data to buffer, marks dirty, then drops BufferMangaer (should call .flush_all())
        let bm = new_buffer_pool(&test_file.0);

        let page_id = bm.allocate_page().unwrap();

        let mut data = bm.get_page_mut(page_id);
        data[0..11].copy_from_slice(b"HELLO THERE");

        drop(data);
        drop(bm);

        // Checks disk for data
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&test_file.0)
            .unwrap();

        let mut dm = DiskManager::new(file, page_id);

        let mut read_buff = [0u8; PAGE_SIZE];
        dm.read_page(page_id, &mut read_buff).unwrap();
        assert_eq!(&read_buff[0..11], b"HELLO THERE");
    }
}
