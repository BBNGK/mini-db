use std::fs::File;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom, Write};

#[derive(Debug)]
#[repr(u8)]
enum PageType {
    DataPage = 1,
    IndexLeaf = 2,
    IndexInternal = 3
}

// Specifies bytes per page
pub const PAGE_SIZE: usize = 4096;

// Specifies the number of pages that can be in buffer at a time
pub const BUFF_POOL_SIZE: usize = 64;

pub struct PageHeader {
    page_type: PageType
}
pub struct Page {
    pub id: u32,
    pub header: PageHeader,
}

pub struct Frame {
    pub page_id: Option<u32>,
    // Holds the page data being caches
    pub data: [u8; PAGE_SIZE],
    // Number of active readers / writers
    pub pin_count: u32,
    // Flag for whether data has been modified by access methods since it was selected from disk
    pub is_dirty: bool
}

pub struct BufferManager {
    // Holds instance of DiskManager
    disk_manager: DiskManager,
    // Holds BUFF_POOL_SIZE pages at a time in cache
    cache: Vec<Frame>,
    // Hashes page_id --> index in cache vector for faster reads
    page_table: HashMap<u32, usize>,
}

pub struct DiskManager {
    file: File,
    num_pages: u32
}

impl DiskManager {
    pub fn new(file: File, num_pages: u32) -> Self {
        Self { file, num_pages }
    }

    // Reads page by calculating offset (page_id * 4096)
    pub fn read_page(&mut self, page_id: u32, buffer: &mut [u8; 4096]) -> std::io::Result<()> {
        // Ensures no overflow from u32 * PAGE_SIZE
        let offset = (page_id as u64) * (PAGE_SIZE as u64);

        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read_exact(buffer)?;

        Ok(())
    }

    // Writes buffer data to page_id
    pub fn write_page(&mut self, page_id: u32, buffer: &[u8; 4096]) -> std::io::Result<()> {
        let offset = (page_id as u64) * (PAGE_SIZE as u64);

        self.file.seek(SeekFrom::Start(offset))?;
        self.file.write_all(buffer)?;

        Ok(())
    }

    // Appends [0u8; 4096] to end of file, returning newly allocated page id
    pub fn allocate_page(&mut self) -> std::io::Result<u32> {
        let new_page_id = self.num_pages;
        let offset = (new_page_id as u64) * (PAGE_SIZE as u64);

        self.file.seek(SeekFrom::Start(offset))?;
        self.file.write_all(&[0u8; PAGE_SIZE])?;
        self.num_pages += 1;

        Ok(new_page_id)
    }
}