#![allow(dead_code)]
use crate::storage::disk::{DiskManager, PAGE_SIZE};
use cyclic_list::List as CLOCKQueue;
use std::collections::HashMap;

// Specifies the number of pages that can be in buffer at a time
pub const BUFF_POOL_SIZE: usize = 64;

// struct size: 32+64*8+32+8 = 584 bits :thumbs_down:
pub struct Frame {
    pub page_id: Option<u32>,
    // Holds the page data being cached.
    pub data: [u8; PAGE_SIZE],
    // Number of active readers / writers.
    pub pin_count: u32,
    // Flag for whether data has been modified by access methods since it was selected from disk.
    pub is_dirty: bool,
}

pub struct BufferManager {
    // Holds instance of DiskManager
    disk_manager: DiskManager,
    // Holds BUFF_POOL_SIZE pages at a time in cache
    cache: CLOCKQueue<(Frame, bool)>, // (Frame, ReferenceBit)
    cache_mra_cursor: u8,             // "most recent access." `None` if cache is empty
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
        let mut cache = CLOCKQueue::new();
        for _ in 0..BUFF_POOL_SIZE {
            cache.push_back((
                Frame {
                    page_id: None,
                    data: [0u8; PAGE_SIZE],
                    pin_count: 0,
                    is_dirty: false,
                },
                false,
            ));
        }

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
        let mut current_cache_line = self.cache.cursor_mut(self.cache_mra_cursor.into());
        let mut traversal_idx = self.cache_mra_cursor;

        while current_cache_line.current().expect("cache: never empty").1 != true {
            current_cache_line
                .current_mut()
                .expect("cache: never empty")
                .1 = false;

            match current_cache_line.move_next() {
                Ok(_) => traversal_idx += 1,
                Err(_) => current_cache_line = self.cache.cursor_start_mut(), // going over ghost node (exists between last and head)
            }
        }

        self.cache_mra_cursor as usize + traversal_idx as usize
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
                let frame = self
                    .cache
                    .cursor_mut(*idx)
                    .current_mut()
                    .expect("No frame at index!");
                frame.0.pin_count += 1;

                &mut frame.0.data
            }
            None => {
                let idx = self.find_replaceable_frame();
                let frame = self
                    .cache
                    .cursor_mut(idx)
                    .current_mut()
                    .expect("No frame at index!");

                // Flushes old page
                if frame.0.is_dirty {
                    self.disk_manager
                        .write_page(frame.0.page_id.unwrap(), &frame.0.data)
                        .expect("Failed to flush dirty page!");
                    frame.0.is_dirty = false;
                }

                if let Some(old_idx) = frame.0.page_id {
                    self.page_table.remove(&old_idx);
                }

                self.page_table.insert(page_id, idx);

                frame.0.pin_count = 1;
                frame.0.page_id = Some(page_id);

                self.disk_manager
                    .read_page(page_id, &mut frame.0.data)
                    .expect("Failed to read new page to frame!");

                &mut frame.0.data
            }
        }
    }

    // Decrements the pin_count by one off a frame, as well as marks as dirty if necessary
    pub fn unpin_page(&mut self, page_id: u32, is_dirty: bool) -> Result<(), &'static str> {
        let frame_idx = self
            .page_table
            .get(&page_id)
            .ok_or("Unpinning page that doesn't exist in cache!")?;

        let frame = self
            .cache
            .cursor_mut(*frame_idx)
            .current_mut()
            .expect("Frame should exist");

        if is_dirty {
            frame.0.is_dirty = true;
        }
        frame.0.pin_count = frame.0.pin_count.saturating_sub(1);

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
