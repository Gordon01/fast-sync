use crate::filesystem::{self, Entry};
use std::ffi::{CStr, c_char, c_void};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::ptr;

const MAX_PATH: usize = 260;
const FILE_ATTRIBUTE_NORMAL: u32 = 0x80;
const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
const INVALID_HANDLE_VALUE: *mut c_void = usize::MAX as *mut c_void;
const FS_FILE_OK: i32 = 0;
const FS_FILE_EXISTS: i32 = 1;
const FS_FILE_NOTFOUND: i32 = 2;
const FS_FILE_WRITEERROR: i32 = 4;

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct FileTime {
    low: u32,
    high: u32,
}

#[repr(C, packed)]
pub struct FindDataA {
    attributes: u32,
    creation_time: FileTime,
    last_access_time: FileTime,
    last_write_time: FileTime,
    size_high: u32,
    size_low: u32,
    reserved0: u32,
    reserved1: u32,
    file_name: [u8; MAX_PATH],
    alternate_file_name: [u8; 14],
}

#[repr(C, packed)]
pub struct FindDataW {
    attributes: u32,
    creation_time: FileTime,
    last_access_time: FileTime,
    last_write_time: FileTime,
    size_high: u32,
    size_low: u32,
    reserved0: u32,
    reserved1: u32,
    file_name: [u16; MAX_PATH],
    alternate_file_name: [u16; 14],
}

struct FindIterator {
    entries: &'static [Entry],
    next_index: usize,
}

type ProgressProc = Option<unsafe extern "C" fn(i32, *mut c_char, *mut c_char, i32) -> i32>;
type LogProc = Option<unsafe extern "C" fn(i32, i32, *mut c_char)>;
type RequestProc =
    Option<unsafe extern "C" fn(i32, i32, *mut c_char, *mut c_char, *mut c_char, i32) -> i32>;
type ProgressProcW = Option<unsafe extern "C" fn(i32, *mut u16, *mut u16, i32) -> i32>;
type LogProcW = Option<unsafe extern "C" fn(i32, i32, *mut u16)>;
type RequestProcW =
    Option<unsafe extern "C" fn(i32, i32, *mut u16, *mut u16, *mut u16, i32) -> i32>;

#[unsafe(no_mangle)]
pub extern "C" fn FsInit(
    _plugin_nr: i32,
    _progress_proc: ProgressProc,
    _log_proc: LogProc,
    _request_proc: RequestProc,
) -> i32 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn FsInitW(
    _plugin_nr: i32,
    _progress_proc: ProgressProcW,
    _log_proc: LogProcW,
    _request_proc: RequestProcW,
) -> i32 {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn FsFindFirst(
    path: *const c_char,
    find_data: *mut FindDataA,
) -> *mut c_void {
    if path.is_null() || find_data.is_null() {
        return INVALID_HANDLE_VALUE;
    }

    let path = unsafe { CStr::from_ptr(path) }.to_string_lossy();
    let Some(entries) = filesystem::list_directory(&path) else {
        return INVALID_HANDLE_VALUE;
    };
    unsafe { write_find_data_a(&mut *find_data, &entries[0]) };
    Box::into_raw(Box::new(FindIterator {
        entries,
        next_index: 1,
    }))
    .cast()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn FsFindFirstW(path: *const u16, find_data: *mut FindDataW) -> *mut c_void {
    if path.is_null() || find_data.is_null() {
        return INVALID_HANDLE_VALUE;
    }

    let Some(path) = (unsafe { read_wide_string(path) }) else {
        return INVALID_HANDLE_VALUE;
    };
    let Some(entries) = filesystem::list_directory(&path) else {
        return INVALID_HANDLE_VALUE;
    };
    unsafe { write_find_data_w(&mut *find_data, &entries[0]) };
    Box::into_raw(Box::new(FindIterator {
        entries,
        next_index: 1,
    }))
    .cast()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn FsFindNext(handle: *mut c_void, find_data: *mut FindDataA) -> i32 {
    if handle.is_null() || handle == INVALID_HANDLE_VALUE || find_data.is_null() {
        return 0;
    }

    let iterator = unsafe { &mut *handle.cast::<FindIterator>() };
    let Some(entry) = iterator.entries.get(iterator.next_index) else {
        return 0;
    };
    unsafe { write_find_data_a(&mut *find_data, entry) };
    iterator.next_index += 1;
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn FsFindNextW(handle: *mut c_void, find_data: *mut FindDataW) -> i32 {
    if handle.is_null() || handle == INVALID_HANDLE_VALUE || find_data.is_null() {
        return 0;
    }

    let iterator = unsafe { &mut *handle.cast::<FindIterator>() };
    let Some(entry) = iterator.entries.get(iterator.next_index) else {
        return 0;
    };
    unsafe { write_find_data_w(&mut *find_data, entry) };
    iterator.next_index += 1;
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn FsFindClose(handle: *mut c_void) -> i32 {
    if !handle.is_null() && handle != INVALID_HANDLE_VALUE {
        drop(unsafe { Box::from_raw(handle.cast::<FindIterator>()) });
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn FsGetFile(
    remote_name: *const c_char,
    local_name: *const c_char,
    copy_flags: i32,
    _remote_info: *mut c_void,
) -> i32 {
    if remote_name.is_null() || local_name.is_null() {
        return FS_FILE_NOTFOUND;
    }

    let remote_name = unsafe { CStr::from_ptr(remote_name) }.to_string_lossy();
    let local_name = unsafe { CStr::from_ptr(local_name) }.to_string_lossy();
    write_virtual_file(&remote_name, Path::new(local_name.as_ref()), copy_flags)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn FsGetFileW(
    remote_name: *const u16,
    local_name: *const u16,
    copy_flags: i32,
    _remote_info: *mut c_void,
) -> i32 {
    let (Some(remote_name), Some(local_name)) =
        (unsafe { read_wide_string(remote_name) }, unsafe {
            read_wide_string(local_name)
        })
    else {
        return FS_FILE_NOTFOUND;
    };
    write_virtual_file(&remote_name, Path::new(&local_name), copy_flags)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn FsGetDefRootName(root_name: *mut c_char, max_len: i32) {
    if root_name.is_null() || max_len <= 0 {
        return;
    }

    let name = b"TotalDroid";
    let copy_len = name.len().min(max_len as usize - 1);
    unsafe {
        ptr::copy_nonoverlapping(name.as_ptr(), root_name.cast::<u8>(), copy_len);
        *root_name.add(copy_len) = 0;
    }
}

fn write_virtual_file(remote_name: &str, local_name: &Path, copy_flags: i32) -> i32 {
    let Some(contents) = filesystem::get_file(remote_name) else {
        return FS_FILE_NOTFOUND;
    };

    let result = if copy_flags & 1 != 0 {
        fs::write(local_name, contents)
    } else {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(local_name)
            .and_then(|mut file| file.write_all(contents))
    };

    match result {
        Ok(()) => FS_FILE_OK,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => FS_FILE_EXISTS,
        Err(_) => FS_FILE_WRITEERROR,
    }
}

fn write_find_data_a(find_data: &mut FindDataA, entry: &Entry) {
    let mut data: FindDataA = unsafe { std::mem::zeroed() };
    data.attributes = if entry.is_directory {
        FILE_ATTRIBUTE_DIRECTORY
    } else {
        FILE_ATTRIBUTE_NORMAL
    };
    if !entry.is_directory {
        data.size_low = entry.contents.len() as u32;
    }
    copy_name_a(entry.name, &mut data.file_name);
    unsafe { ptr::write(find_data, data) };
}

fn write_find_data_w(find_data: &mut FindDataW, entry: &Entry) {
    let mut data: FindDataW = unsafe { std::mem::zeroed() };
    data.attributes = if entry.is_directory {
        FILE_ATTRIBUTE_DIRECTORY
    } else {
        FILE_ATTRIBUTE_NORMAL
    };
    if !entry.is_directory {
        data.size_low = entry.contents.len() as u32;
    }
    let mut file_name = [0; MAX_PATH];
    copy_name_w(entry.name, &mut file_name);
    data.file_name = file_name;
    unsafe { ptr::write(find_data, data) };
}

fn copy_name_a(name: &str, output: &mut [u8; MAX_PATH]) {
    let bytes = name.as_bytes();
    let copy_len = bytes.len().min(output.len() - 1);
    output[..copy_len].copy_from_slice(&bytes[..copy_len]);
}

fn copy_name_w(name: &str, output: &mut [u16; MAX_PATH]) {
    let encoded: Vec<u16> = name.encode_utf16().collect();
    let copy_len = encoded.len().min(output.len() - 1);
    output[..copy_len].copy_from_slice(&encoded[..copy_len]);
}

unsafe fn read_wide_string(value: *const u16) -> Option<String> {
    if value.is_null() {
        return None;
    }

    let mut length = 0;
    while length < 32_768 && unsafe { *value.add(length) } != 0 {
        length += 1;
    }
    if length == 32_768 {
        return None;
    }
    Some(String::from_utf16_lossy(unsafe {
        std::slice::from_raw_parts(value, length)
    }))
}
