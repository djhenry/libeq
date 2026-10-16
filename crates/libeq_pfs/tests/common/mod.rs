use std::io::{Cursor, Write};

use flate2::{Compression, write::ZlibEncoder};
use libeq_pfs::PfsReader;

pub const BLOCK_SIZE: usize = 8192;
pub const MEMBER: &str = "sample.dat";
const MEMBER_CRC: u32 = 0xbef6_4eaa;
const DIRECTORY_CRC: u32 = 0x6158_0ac9;

pub struct Block {
    declared: u32,
    pub compressed: Vec<u8>,
}

impl Block {
    pub fn new(declared: u32, data: &[u8]) -> Self {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(data).unwrap();
        Self {
            declared,
            compressed: encoder.finish().unwrap(),
        }
    }

    fn append(&self, archive: &mut Vec<u8>) {
        archive.extend_from_slice(&(self.compressed.len() as u32).to_le_bytes());
        archive.extend_from_slice(&self.declared.to_le_bytes());
        archive.extend_from_slice(&self.compressed);
    }
}

pub fn directory() -> Vec<u8> {
    let mut bytes = 1u32.to_le_bytes().to_vec();
    bytes.extend_from_slice(&11u32.to_le_bytes());
    bytes.extend_from_slice(b"sample.dat\0");
    bytes
}

pub fn archive(blocks: &[Block], directory: Block) -> PfsReader<Cursor<Vec<u8>>> {
    let mut bytes = vec![0; 12];
    let member_size: u32 = blocks.iter().map(|block| block.declared).sum();
    for block in blocks {
        block.append(&mut bytes);
    }
    let directory_offset = bytes.len() as u32;
    directory.append(&mut bytes);
    let index_offset = bytes.len() as u32;
    for word in [
        2,
        MEMBER_CRC,
        12,
        member_size,
        DIRECTORY_CRC,
        directory_offset,
        directory.declared,
    ] {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes[..4].copy_from_slice(&index_offset.to_le_bytes());
    bytes[4..8].copy_from_slice(b"PFS ");
    bytes[8..12].copy_from_slice(&0x20000u32.to_le_bytes());
    PfsReader::open(Cursor::new(bytes)).unwrap()
}

pub fn member_archive(blocks: &[Block]) -> PfsReader<Cursor<Vec<u8>>> {
    let directory = directory();
    archive(blocks, Block::new(directory.len() as u32, &directory))
}
