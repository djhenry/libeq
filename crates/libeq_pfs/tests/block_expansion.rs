mod common;

use std::io::{ErrorKind, Read};

use common::{BLOCK_SIZE, Block, MEMBER, archive, directory, member_archive};
use libeq_pfs::Error;

fn assert_invalid_data(error: Error) {
    match error {
        Error::IO(error) => assert_eq!(error.kind(), ErrorKind::InvalidData),
        other => panic!("expected invalid decompressed data, got {other}"),
    }
}

#[test]
fn valid_empty_small_maximum_and_multiblock_members_round_trip() {
    for expected in [
        vec![],
        vec![0x31],
        vec![0x52; BLOCK_SIZE],
        vec![0x73; BLOCK_SIZE + 7],
    ] {
        let blocks: Vec<_> = expected
            .chunks(BLOCK_SIZE)
            .map(|chunk| Block::new(chunk.len() as u32, chunk))
            .collect();
        let mut archive = member_archive(&blocks);
        assert_eq!(archive.filenames().unwrap(), [MEMBER]);
        let info = archive.info(MEMBER).unwrap().unwrap();
        assert_eq!(info.uncompressed_size as usize, expected.len());
        assert_eq!(info.block_count as usize, blocks.len());
        let mut actual = Vec::new();
        archive
            .get_reader(MEMBER)
            .unwrap()
            .unwrap()
            .read_to_end(&mut actual)
            .unwrap();
        assert_eq!(actual, expected);
        assert_eq!(archive.get(MEMBER).unwrap().unwrap(), expected);
    }
}

#[test]
fn oversized_first_block_is_rejected_during_reader_creation() {
    let mut archive = member_archive(&[Block::new(8192, &vec![0x61; 1 << 20])]);
    assert_eq!(
        archive.info(MEMBER).unwrap().unwrap().uncompressed_size,
        8192
    );
    let error = match archive.get_reader(MEMBER) {
        Err(error) => error,
        Ok(_) => panic!("oversized eager block was accepted"),
    };
    assert_invalid_data(error);
}

#[test]
fn undersized_first_block_is_rejected_during_reader_creation() {
    let mut archive = member_archive(&[Block::new(8192, b"short")]);
    let error = match archive.get_reader(MEMBER) {
        Err(error) => error,
        Ok(_) => panic!("undersized eager block was accepted"),
    };
    assert_invalid_data(error);
}

fn check_later_block(actual: &[u8]) {
    let mut archive = member_archive(&[
        Block::new(8192, &vec![0x11; BLOCK_SIZE]),
        Block::new(7, actual),
    ]);
    let mut reader = archive.get_reader(MEMBER).unwrap().unwrap();
    let mut first = vec![0; BLOCK_SIZE];
    reader.read_exact(&mut first).unwrap();
    assert_eq!(first, vec![0x11; BLOCK_SIZE]);
    let mut remainder = Vec::new();
    let error = reader.read_to_end(&mut remainder).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidData);
    assert!(
        remainder.is_empty(),
        "invalid block bytes must not be exposed"
    );
}

#[test]
fn oversized_later_block_is_rejected_when_reached() {
    check_later_block(&vec![0x22; 1 << 20]);
}

#[test]
fn undersized_later_block_is_rejected_when_reached() {
    check_later_block(b"short");
}

#[test]
fn get_rejects_expansion_mismatch_in_first_and_later_blocks() {
    for actual in [vec![0x33; 1 << 20], b"short".to_vec()] {
        for prefix in [false, true] {
            let mut blocks = Vec::new();
            if prefix {
                blocks.push(Block::new(8192, &vec![0x11; BLOCK_SIZE]));
            }
            blocks.push(Block::new(7, &actual));
            let error = match member_archive(&blocks).get(MEMBER) {
                Err(error) => error,
                Ok(_) => panic!("get accepted a block with mismatched expansion"),
            };
            assert_invalid_data(error);
        }
    }
}

#[test]
fn filenames_rejects_expansion_mismatch_in_directory() {
    let valid = directory();
    let mut oversized = valid.clone();
    oversized.resize(1 << 20, 0x44);
    for actual in [oversized, valid[..valid.len() - 1].to_vec()] {
        let mut archive = archive(&[], Block::new(valid.len() as u32, &actual));
        assert_invalid_data(archive.filenames().unwrap_err());
    }
}

fn corrupt_block() -> Block {
    let mut block = Block::new(7, b"payload");
    *block.compressed.last_mut().unwrap() ^= 1;
    block
}

#[test]
fn matching_size_does_not_bypass_first_block_checksum_error() {
    let mut archive = member_archive(&[corrupt_block()]);
    let error = match archive.get_reader(MEMBER) {
        Err(error) => error,
        Ok(_) => panic!("corrupt eager block was accepted"),
    };
    assert!(matches!(error, Error::IO(_)));
}

#[test]
fn matching_size_does_not_bypass_later_block_checksum_error() {
    let mut archive = member_archive(&[Block::new(8192, &vec![0x11; BLOCK_SIZE]), corrupt_block()]);
    let mut reader = archive.get_reader(MEMBER).unwrap().unwrap();
    let mut actual = Vec::new();
    assert!(reader.read_to_end(&mut actual).is_err());
    assert_eq!(actual, vec![0x11; BLOCK_SIZE]);
}
