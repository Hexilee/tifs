use super::tikv_fs::TiFs;

type Block = Vec<u8>;

pub fn empty_block() -> Block {
    vec![0; TiFs::BLOCK_SIZE as usize]
}
