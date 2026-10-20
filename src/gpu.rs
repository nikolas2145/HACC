#[cfg(feature = "gpu")]
mod ffi {
    use std::os::raw::{c_int, c_uchar, c_uint};

    extern "C" {
        pub fn gpu_compress_block(
            input: *const c_uchar,
            input_len: c_uint,
            output: *mut c_uchar,
            output_capacity: c_uint,
            output_len: *mut c_uint,
        ) -> c_int;

        pub fn gpu_decompress_block(
            input: *const c_uchar,
            input_len: c_uint,
            output: *mut c_uchar,
            output_capacity: c_uint,
            output_len: *mut c_uint,
        ) -> c_int;
    }

    pub const GPU_OK: c_int = 0;
}

#[cfg(feature = "gpu")]
pub fn gpu_compress(input: &[u8]) -> Result<Vec<u8>, String> {
    use std::os::raw::c_uint;

    let in_len = input.len() as c_uint;

    // RLE chunks can expand; 2x is above the format's worst case.
    let mut out = vec![0u8; input.len() * 2 + 1024];
    let mut out_len: c_uint = 0;

    let rc = unsafe {
        ffi::gpu_compress_block(
            input.as_ptr(),
            in_len,
            out.as_mut_ptr(),
            out.len() as c_uint,
            &mut out_len as *mut c_uint,
        )
    };

    if rc != ffi::GPU_OK {
        return Err(format!(
            "GPU compression failed (code {rc}; 2 means CUDA device/runtime error)"
        ));
    }

    if out_len as usize > out.len() {
        return Err("gpu_compress_block returned an invalid size".into());
    }
    out.truncate(out_len as usize);
    Ok(out)
}

#[cfg(feature = "gpu")]
pub fn gpu_decompress(input: &[u8], expected_len: usize) -> Result<Vec<u8>, String> {
    use std::os::raw::c_uint;

    let in_len = input.len() as c_uint;
    let mut out = vec![0u8; expected_len];
    let mut out_len: c_uint = 0;

    let rc = unsafe {
        ffi::gpu_decompress_block(
            input.as_ptr(),
            in_len,
            out.as_mut_ptr(),
            out.len() as c_uint,
            &mut out_len as *mut c_uint,
        )
    };

    if rc != ffi::GPU_OK {
        return Err(format!(
            "GPU decompression failed (code {rc}; 2 means CUDA device/runtime error)"
        ));
    }

    if out_len as usize != expected_len {
        return Err(format!(
            "gpu_decompress_block size mismatch: got {}, expected {}",
            out_len, expected_len
        ));
    }

    Ok(out)
}

#[cfg(not(feature = "gpu"))]
pub fn gpu_compress(_input: &[u8]) -> Result<Vec<u8>, String> {
    Err("GPU support not compiled (feature `gpu` disabled)".into())
}

#[cfg(not(feature = "gpu"))]
pub fn gpu_decompress(_input: &[u8], _expected_len: usize) -> Result<Vec<u8>, String> {
    Err("GPU support not compiled (feature `gpu` disabled)".into())
}
