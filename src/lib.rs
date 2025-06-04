//! epub_emoji_x 库主入口

pub mod replacer;

use std::ffi::CStr;
use crate::replacer::replace_emoji_in_epub_impl;

// 可供 FFI 调用的接口示例
#[no_mangle]
pub extern "C" fn replace_emoji_in_epub(input_path: *const std::os::raw::c_char, output_path: *const std::os::raw::c_char) -> i32 {
    let input = unsafe { CStr::from_ptr(input_path) }.to_string_lossy();
    let output = unsafe { CStr::from_ptr(output_path) }.to_string_lossy();
    match replace_emoji_in_epub_impl(&input, &output) {
        Ok(_) => 0,
        Err(_) => 1,
    }
}
