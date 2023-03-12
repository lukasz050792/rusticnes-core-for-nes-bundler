use std::{convert::TryInto};

pub(crate) fn save_usize(buff: &mut Vec<u8>, data: usize) {
    buff.extend(&data.to_le_bytes());
}
pub(crate) fn load_usize(buff: &mut Vec<u8>, data: &mut usize) {
    *data = usize::from_le_bytes(buff.split_off(buff.len() - std::mem::size_of::<usize>()).try_into().unwrap())
}

pub(crate) fn save_u8(buff: &mut Vec<u8>, data: u8) {
    buff.push(data);
}
pub(crate) fn load_u8(buff: &mut Vec<u8>, data: &mut u8) {
    *data = buff.pop().unwrap()
}

pub(crate) fn save_u16(buff: &mut Vec<u8>, data: u16) {
    buff.extend(data.to_le_bytes());
}
pub(crate) fn load_u16(buff: &mut Vec<u8>, data: &mut u16) {
    *data = u16::from_le_bytes(buff.split_off(buff.len() - std::mem::size_of::<u16>()).try_into().unwrap())
}

pub(crate) fn save_u32(buff: &mut Vec<u8>, data: u32) {
    buff.extend(data.to_le_bytes());
}
pub(crate) fn load_u32(buff: &mut Vec<u8>, data: &mut u32) {
    *data = u32::from_le_bytes(buff.split_off(buff.len() - std::mem::size_of::<u32>()).try_into().unwrap())
}

pub(crate) fn save_u64(buff: &mut Vec<u8>, data: u64) {
    buff.extend(data.to_le_bytes());
}
pub(crate) fn load_u64(buff: &mut Vec<u8>, data: &mut u64) {
    *data = u64::from_le_bytes(buff.split_off(buff.len() - std::mem::size_of::<u64>()).try_into().unwrap())
}
pub(crate) fn save_bool(buff: &mut Vec<u8>, data: bool) {
    save_u8(buff, data as u8);
}
pub(crate) fn load_bool(buff: &mut Vec<u8>, data: &mut bool) {
    *data = buff.pop().unwrap() != 0
}

pub(crate) fn save_vec(buff: &mut Vec<u8>, data: &Vec<u8>) {
    buff.extend(data);
}
pub(crate) fn load_vec(buff: &mut Vec<u8>, data: &mut Vec<u8>) {
    *data = buff.split_off(buff.len() - data.len())
}

pub(crate) fn save_vec_usize(buff: &mut Vec<u8>, data: &Vec<usize>) {
    for d in data {
        save_usize(buff, *d);
    }
}
pub(crate) fn load_vec_usize(buff: &mut Vec<u8>, data: &mut Vec<usize>) {
    for d in &mut data.iter_mut().rev() {
        load_usize(buff, d);
    }
}