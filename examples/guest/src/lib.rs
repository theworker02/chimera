//! Sample Wasm guest for Chimera compute slices.
//!
//! ABI: `chimera_alloc` / `chimera_dealloc` / `chimera_execute`

#[no_mangle]
pub extern "C" fn chimera_alloc(len: u32) -> u32 {
    let layout = std::alloc::Layout::from_size_align(len as usize, 8)
        .unwrap_or_else(|_| std::alloc::Layout::from_size_align(1, 1).unwrap());
    unsafe {
        let ptr = std::alloc::alloc(layout);
        if ptr.is_null() {
            0
        } else {
            ptr as u32
        }
    }
}

#[no_mangle]
pub extern "C" fn chimera_dealloc(ptr: u32, len: u32) {
    if ptr == 0 || len == 0 {
        return;
    }
    let Ok(layout) = std::alloc::Layout::from_size_align(len as usize, 8) else {
        return;
    };
    unsafe { std::alloc::dealloc(ptr as *mut u8, layout) }
}

#[no_mangle]
pub extern "C" fn chimera_execute(in_ptr: u32, in_len: u32, out_ptr: u32, out_cap: u32) -> i32 {
    if in_ptr == 0 || out_ptr == 0 || in_len < 16 || out_cap < 16 {
        return -1;
    }
    unsafe {
        let input = std::slice::from_raw_parts(in_ptr as *const u8, in_len as usize);
        let output = std::slice::from_raw_parts_mut(out_ptr as *mut u8, out_cap as usize);
        let seed = u64::from_le_bytes(input[0..8].try_into().unwrap_or([0; 8]));
        let count = u32::from_le_bytes(input[8..12].try_into().unwrap_or([0; 4])) as usize;
        let needed = 16 + count * 4;
        if input.len() < needed || output.len() < needed {
            return -2;
        }
        let mut checksum: u64 = seed ^ 0xC0FFEE_u64;
        for i in 0..count {
            let base = 16 + i * 4;
            let mut bytes = [0u8; 4];
            bytes.copy_from_slice(&input[base..base + 4]);
            let v = f32::from_le_bytes(bytes);
            let out = (v * 1.618_034).sin().abs() * (1.0 + (i as f32) * 0.000_1);
            output[base..base + 4].copy_from_slice(&out.to_le_bytes());
            checksum = checksum
                .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                .wrapping_add(out.to_bits() as u64);
        }
        output[0..8].copy_from_slice(&checksum.to_le_bytes());
        output[8..12].copy_from_slice(&(count as u32).to_le_bytes());
        output[12..16].fill(0);
        needed as i32
    }
}
