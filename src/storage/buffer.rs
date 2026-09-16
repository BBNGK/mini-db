#![allow(dead_code)]
use crate::storage::disk::{DiskManager, PAGE_SIZE};
use std::collections::HashMap;

// Specifies the number of pages that can be in buffer at a time
pub const BUFF_POOL_SIZE: usize = 64;

// struct size: 32+4096*8+32+8 = 32,840 bits :thumbs_down:
#[derive(Clone)]
pub struct Frame {
    pub page_id: Option<u32>,
    // Holds the page data being cached.
    pub data: [u8; PAGE_SIZE],
    // Number of active readers / writers.
    pub pin_count: u32,
    // Flag for whether data has been modified by access methods since it was selected from disk.
    pub is_dirty: bool,
}

impl std::default::Default for Frame {
    fn default() -> Self {
        Self {
            page_id: None,
            data: [0; PAGE_SIZE],
            pin_count: 0,
            is_dirty: false,
        }
    }
}

pub struct BufferManager {
    disk_manager: DiskManager,
    cache: [(Frame, bool); BUFF_POOL_SIZE], // (Frame, ReferenceBit)
    cache_mra_cursor: u16, // most recent access. NOTE(ansh): probably poor cache locality. mark for review later.
    page_table: HashMap<u32, usize>, // Hashes page_id --> index in cache vector for faster reads
}

impl Drop for BufferManager {
    fn drop(&mut self) {
        let _ = self.flush_all();
    }
}

impl BufferManager {
    pub fn new(disk_manager: DiskManager) -> Self {
        let cache: [(Frame, bool); BUFF_POOL_SIZE] = core::array::from_fn(|_| {
            (
                Frame {
                    page_id: None,
                    data: [0u8; PAGE_SIZE],
                    pin_count: 0,
                    is_dirty: false,
                },
                false,
            )
        });

        Self {
            disk_manager,
            cache,
            cache_mra_cursor: 0,
            page_table: HashMap::new(),
        }
    }

    // "Forwarding" allocation function for EE / Access Methods to call
    pub fn allocate_page(&mut self) -> std::io::Result<u32> {
        self.disk_manager.allocate_page()
    }

    // Helper fn for finding suitable Frame idx to be replaced.
    //
    // TEMPORARY eviction policy, was too lazy to look into LRU / CLOCK. This at least works for testing purposes,
    // but just panics when at max capacity...
    //
    // Also most definitely thrashes all over the place in a real setting....
    //
    // NOTE(ansh): currently CLOCK. might need to be upgraded later.
    // TODO(ansh): copy list to traverse. currently mutates correct cache.
    // https://www.josehu.com/technical/2020/08/07/cache-eviction-algorithms.html
    pub fn find_replaceable_frame(&mut self) -> usize {
        // NOTE(ansh): store the original cache idx and the ref bit corresponding to it.
        let mut reference_bits: Vec<(usize, bool)> = self
            .cache
            .iter()
            .enumerate()
            .filter_map(|(idx, cl)| match cl.0.pin_count == 0 {
                true => Some((idx, cl.1)),
                false => None,
            })
            .collect();
        let mut current_cursor = self.cache_mra_cursor as usize;

        while reference_bits[current_cursor].1 {
            reference_bits[current_cursor].1 = false;

            current_cursor = reference_bits[current_cursor].0;
            current_cursor %= BUFF_POOL_SIZE;
        }

        current_cursor
    }

    // Handles obtaining page information from cache.
    //
    // If the page doesn't exist in cache, requests page from DiskManager and writes the page
    // to an available Frame in cache.
    //
    // Frames are replaced when they either have a pin_count of 0 or no page_id assigned, while also
    // making sure to flush dirty data back to disk before replacing.
    //
    pub fn get_page(&mut self, page_id: u32) -> &mut [u8; PAGE_SIZE] {
        match self.page_table.get(&page_id) {
            Some(idx) => {
                let frame = &mut self.cache[*idx].0;
                frame.pin_count += 1;

                &mut frame.data
            }
            None => {
                let idx = self.find_replaceable_frame();
                let frame = &mut self.cache[idx].0;

                // Flushes old page
                if frame.is_dirty {
                    self.disk_manager
                        .write_page(frame.page_id.unwrap(), &frame.data)
                        .expect("Failed to flush dirty page!");
                    frame.is_dirty = false;
                }

                if let Some(old_idx) = frame.page_id {
                    self.page_table.remove(&old_idx);
                }

                self.page_table.insert(page_id, idx);

                frame.pin_count = 1;
                frame.page_id = Some(page_id);

                self.disk_manager
                    .read_page(page_id, &mut frame.data)
                    .expect("Failed to read new page to frame!");

                &mut frame.data
            }
        }
    }

    // Decrements the pin_count by one off a frame, as well as marks as dirty if necessary
    pub fn unpin_page(&mut self, page_id: u32, is_dirty: bool) -> Result<(), &'static str> {
        let frame_idx = self
            .page_table
            .get(&page_id)
            .ok_or("Unpinning page that doesn't exist in cache!")?;

        let frame = &mut self.cache[*frame_idx].0;

        if is_dirty {
            frame.is_dirty = true;
        }
        frame.pin_count = frame.pin_count.saturating_sub(1);

        Ok(())
    }

    // Call to flush all dirty frames into disk.
    //
    // Useful for when BufferManager is Dropped (potential crash?), as well as for
    // "checkpointing" (future WAL / background thread for "cleanup"?).
    //
    pub fn flush_all(&mut self) -> Result<(), std::io::Error> {
        for frame in self.cache.iter_mut() {
            if frame.0.is_dirty {
                self.disk_manager.write_page(
                    frame.0.page_id.expect("Page is dirty w/o page_id?"),
                    &frame.0.data,
                )?;
                frame.0.is_dirty = false;
            }
        }

        self.disk_manager.sync()?;
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
            .create(true)
            .open(&temp_file.0)?;
        let disk_mgr = DiskManager::new(temp_file, 32);
        let mut buf_mgr = BufferManager::new(disk_mgr);

        // test: check eviction of first page in default config.

        let replaceable_cache_idx = buf_mgr.find_replaceable_frame();
        assert_eq!(replaceable_cache_idx, 0);

        // test: replace with set configuration and test for first zero ref bit.

        const TEST_FRAMES_COUNT: usize = 4;
        for idx in 0..TEST_FRAMES_COUNT {
            // [
            //  (Frame, 1),
            //  (Frame, 1),
            //  (Frame, 0),
            //  (Frame, 1)
            //            ]
            buf_mgr.cache[idx] = (Frame::default(), idx != 2)
        }
        let replaceable_cache_idx = buf_mgr.find_replaceable_frame();
        assert_eq!(replaceable_cache_idx, 2);

        // test: all ref bits = 1.

        for idx in 0..BUFF_POOL_SIZE {
            // [
            //  (Frame, 1),
            //  (Frame, 1),
            //  (Frame, 1),
            //  (Frame, 1),
            //       .
            //       .
            //       .
            //  (Frame, 1)
            //            ]
            buf_mgr.cache[idx] = (Frame::default(), true)
        }
        buf_mgr.cache_mra_cursor = 1;
        let replaceable_cache_idx = buf_mgr.find_replaceable_frame();
        assert_eq!(replaceable_cache_idx, 1);

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
        let mut bm = new_buffer_pool(&test_file.0);

        let page_id = bm.disk_manager.allocate_page().unwrap();

        {
            let data = bm.get_page(page_id);
            data[0..11].copy_from_slice("HELLO THERE".as_bytes());
        }

        bm.unpin_page(page_id, true).unwrap();

        {
            let reread_data = bm.get_page(page_id);
            assert_eq!(&reread_data[0..11], "HELLO THERE".as_bytes());
        }

        bm.unpin_page(page_id, false).unwrap();
    }

    #[test]
    fn test_flush_writes_to_disk() {
        let test_file = TmpFile::new("test_flush_writes_to_disk");

        // Writes data to buffer, marks dirty, then drops BufferMangaer (should call .flush_all())
        let mut bm = new_buffer_pool(&test_file.0);

        let page_id = bm.disk_manager.allocate_page().unwrap();

        let data = bm.get_page(page_id);
        data[0..11].copy_from_slice("HELLO THERE".as_bytes());

        bm.unpin_page(page_id, true).unwrap();

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
        assert_eq!(&read_buff[0..11], "HELLO THERE".as_bytes());
    }
}
