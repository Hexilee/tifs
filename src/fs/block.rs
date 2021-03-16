type Block = Vec<u8>;

pub fn empty_block(block_size: u64) -> Block {
    vec![0; block_size as usize]
}
