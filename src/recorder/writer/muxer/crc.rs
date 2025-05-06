// Include of crc.rs from RustAudio/ogg

// Ogg decoder and encoder written in Rust
//
// Copyright (c) 2016-2017 est31 <MTest31@outlook.com>
// and contributors. All rights reserved.
// Redistribution or use only under the terms
// specified in the LICENSE file attached to this
// source distribution.

/*!
Implementation of the CRC algorithm with the
vorbis specific parameters and setup
*/

// Lookup table to enable bytewise CRC32 calculation
static CRC_LOOKUP_ARRAY :&[u32] = &lookup_array();

const fn get_tbl_elem(idx :u32) -> u32 {
    let mut r :u32 = idx << 24;
    let mut i = 0;
    while i < 8 {
        r = (r << 1) ^ (-(((r >> 31) & 1) as i32) as u32 & 0x04c11db7);
        i += 1;
    }
    return r;
}

const fn lookup_array() -> [u32; 0x100] {
    let mut lup_arr :[u32; 0x100] = [0; 0x100];
    let mut i = 0;
    while i < 0x100 {
        lup_arr[i] = get_tbl_elem(i as u32);
        i += 1;
    }
    lup_arr
}

pub fn vorbis_crc32(array :&[u8]) -> u32 {
    return vorbis_crc32_update(0, array);
}

pub fn vorbis_crc32_update(cur :u32, array :&[u8]) -> u32 {
    let mut ret :u32 = cur;
    for av in array {
        ret = (ret << 8) ^ CRC_LOOKUP_ARRAY[(*av as u32 ^ (ret >> 24)) as usize];
    }
    return ret;
}