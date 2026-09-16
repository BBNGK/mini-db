use crate::storage::disk::{DiskManager, PAGE_SIZE};
use std::collections::HashMap;

// Specifies the number of pages that can be in buffer at a time
pub const BUFF_POOL_SIZE: usize = 64;

pub struct Frame {
    pub page_id: Option<u32>,
    // Holds the page data being caches
    pub data: [u8; PAGE_SIZE],
    // Number of active readers / writers
    pub pin_count: u32,
    // Flag for whether data has been modified by access methods since it was selected from disk
    pub is_dirty: bool,
}

//
//
//
pub struct BufferManager {
    // Holds instance of DiskManager
    disk_manager: DiskManager,
    // Holds BUFF_POOL_SIZE pages at a time in cache
    cache: Vec<Frame>,
    // NOTE(ansh): probably poor cache locality. mark for review later.
    // Hashes page_id --> index in cache vector for faster reads
    page_table: HashMap<u32, usize>,
}

impl Drop for BufferManager {
    fn drop(&mut self) {
        let _ = self.flush_all();
    }
}

impl BufferManager {
    pub fn new(disk_manager: DiskManager) -> Self {
        let mut cache = Vec::with_capacity(BUFF_POOL_SIZE);
        for _ in 0..BUFF_POOL_SIZE {
            cache.push(Frame {
                page_id: None,
                data: [0u8; PAGE_SIZE],
                pin_count: 0,
                is_dirty: false,
            });
        }

        Self {
            disk_manager,
            cache,
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
    pub fn find_replacable_frame(&self) -> usize {
        if let Some((idx, _)) = self
            .cache
            .iter()
            .enumerate()
            .find(|(_, c)| c.page_id.is_none())
        {
            return idx;
        };

        if let Some((idx, _)) = self
            .cache
            .iter()
            .enumerate()
            .find(|(_, c)| c.pin_count == 0)
        {
            return idx;
        }

        panic!("Out of cache space! No frames available!");
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
                let frame = self.cache.get_mut(*idx).expect("No frame at index!");
                frame.pin_count += 1;

                &mut frame.data
            }
            None => {
                let idx = self.find_replacable_frame();
                let frame = self.cache.get_mut(idx).expect("No frame at index!");

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

        let frame = self.cache.get_mut(*frame_idx).expect("Frame should exist");

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
            if frame.is_dirty {
                self.disk_manager.write_page(
                    frame.page_id.expect("Page is dirty w/o page_id?"),
                    &frame.data,
                )?;
                frame.is_dirty = false;
            }
        }

        self.disk_manager.sync()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs::OpenOptions;
    use std::path::PathBuf;
    use super::*;
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

    fn new_buffer_pool(file_path: &PathBuf) -> BufferManager {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(file_path).expect("Couldn't open test file!");

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
