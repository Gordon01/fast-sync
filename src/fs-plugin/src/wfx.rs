use crate::filesystem::{self, Entry, FileError};
use fast_sync::wft::Error as WftError;
use std::ffi::{CStr, c_char, c_void};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::ptr;

const MAX_PATH: usize = 260;
const FILE_ATTRIBUTE_NORMAL: u32 = 0x80;
const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
const INVALID_HANDLE_VALUE: *mut c_void = usize::MAX as *mut c_void;
const FS_FILE_OK: i32 = 0;
const FS_FILE_EXISTS: i32 = 1;
const FS_FILE_NOTFOUND: i32 = 2;
const FS_FILE_READERROR: i32 = 3;
const FS_FILE_WRITEERROR: i32 = 4;

#[repr(C, packed)]
#[derive(Clone, Copy, Default)]
struct FileTime {
    low: u32,
    high: u32,
}

#[repr(C, packed)]
pub struct FindData<C> {
    attributes: u32,
    creation_time: FileTime,
    last_access_time: FileTime,
    last_write_time: FileTime,
    size_high: u32,
    size_low: u32,
    reserved0: u32,
    reserved1: u32,
    file_name: [C; MAX_PATH],
    alternate_file_name: [C; 14],
}

pub type FindDataA = FindData<u8>;
pub type FindDataW = FindData<u16>;

pub trait Char: Copy + Default {
    fn encode(name: &str) -> Vec<Self>;
}

impl Char for u8 {
    fn encode(name: &str) -> Vec<Self> {
        name.as_bytes().to_vec()
    }
}

impl Char for u16 {
    fn encode(name: &str) -> Vec<Self> {
        name.encode_utf16().collect()
    }
}

impl<C: Char> FindData<C> {
    fn new(entry: &Entry) -> Self {
        let size = if entry.is_directory { 0 } else { entry.size };
        let mut file_name = [C::default(); MAX_PATH];
        let encoded = C::encode(&entry.name);
        let copy_len = encoded.len().min(MAX_PATH - 1);
        file_name[..copy_len].copy_from_slice(&encoded[..copy_len]);
        Self {
            attributes: if entry.is_directory {
                FILE_ATTRIBUTE_DIRECTORY
            } else {
                FILE_ATTRIBUTE_NORMAL
            },
            creation_time: FileTime::default(),
            last_access_time: FileTime::default(),
            last_write_time: FileTime::default(),
            size_high: (size >> 32) as u32,
            size_low: size as u32,
            reserved0: 0,
            reserved1: 0,
            file_name,
            alternate_file_name: [C::default(); 14],
        }
    }
}

struct FindIterator {
    entries: Vec<Entry>,
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
    initialize_plugin()
}

#[unsafe(no_mangle)]
pub extern "C" fn FsInitW(
    _plugin_nr: i32,
    _progress_proc: ProgressProcW,
    _log_proc: LogProcW,
    _request_proc: RequestProcW,
) -> i32 {
    initialize_plugin()
}

fn initialize_plugin() -> i32 {
    if let Err(error) = initialize_logging() {
        eprintln!("android-wfx: {error}");
        return 1;
    }
    tracing::info!("initializing WFT filesystem plugin");
    match filesystem::initialize() {
        Ok(()) => {
            tracing::info!("WFT filesystem plugin initialized");
            0
        }
        Err(error) => {
            tracing::error!(%error, "WFT filesystem plugin initialization failed");
            1
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn FsFindFirst(
    path: *const c_char,
    find_data: *mut FindDataA,
) -> *mut c_void {
    unsafe { find_first(read_c_string(path), find_data) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn FsFindFirstW(path: *const u16, find_data: *mut FindDataW) -> *mut c_void {
    unsafe { find_first(read_wide_string(path), find_data) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn FsFindNext(handle: *mut c_void, find_data: *mut FindDataA) -> i32 {
    unsafe { find_next(handle, find_data) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn FsFindNextW(handle: *mut c_void, find_data: *mut FindDataW) -> i32 {
    unsafe { find_next(handle, find_data) }
}

unsafe fn find_first<C: Char>(path: Option<String>, find_data: *mut FindData<C>) -> *mut c_void {
    let Some(path) = path.filter(|_| !find_data.is_null()) else {
        return INVALID_HANDLE_VALUE;
    };
    let _span = tracing::info_span!("find_first", path = %path).entered();
    let entries = match filesystem::list_directory(&path) {
        Ok(entries) => entries,
        Err(error) => {
            tracing::error!(%error, "failed to list WFX directory");
            return INVALID_HANDLE_VALUE;
        }
    };
    let Some(first) = entries.first() else {
        return INVALID_HANDLE_VALUE;
    };
    unsafe { find_data.write(FindData::new(first)) };
    Box::into_raw(Box::new(FindIterator {
        entries,
        next_index: 1,
    }))
    .cast()
}

unsafe fn find_next<C: Char>(handle: *mut c_void, find_data: *mut FindData<C>) -> i32 {
    if handle.is_null() || handle == INVALID_HANDLE_VALUE || find_data.is_null() {
        return 0;
    }

    let iterator = unsafe { &mut *handle.cast::<FindIterator>() };
    let Some(entry) = iterator.entries.get(iterator.next_index) else {
        return 0;
    };
    unsafe { find_data.write(FindData::new(entry)) };
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
    let (Some(remote_name), Some(local_name)) =
        (unsafe { (read_c_string(remote_name), read_c_string(local_name)) })
    else {
        return FS_FILE_NOTFOUND;
    };
    write_remote_file(&remote_name, Path::new(&local_name), copy_flags)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn FsGetFileW(
    remote_name: *const u16,
    local_name: *const u16,
    copy_flags: i32,
    _remote_info: *mut c_void,
) -> i32 {
    let (Some(remote_name), Some(local_name)) =
        (unsafe { (read_wide_string(remote_name), read_wide_string(local_name)) })
    else {
        return FS_FILE_NOTFOUND;
    };
    write_remote_file(&remote_name, Path::new(&local_name), copy_flags)
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

#[tracing::instrument(
    name = "get_file",
    skip_all,
    fields(remote_path = %remote_name, local_path = %local_name.display(), copy_flags)
)]
fn write_remote_file(remote_name: &str, local_name: &Path, copy_flags: i32) -> i32 {
    let contents = match filesystem::get_file(remote_name) {
        Ok(contents) => contents,
        Err(error) => {
            tracing::error!(%error, "failed to download remote file");
            return if matches!(
                error,
                FileError::Wft(WftError::HttpStatus { status: 404, .. })
            ) {
                FS_FILE_NOTFOUND
            } else {
                FS_FILE_READERROR
            };
        }
    };

    let result = if copy_flags & 1 != 0 {
        fs::write(local_name, contents)
    } else {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(local_name)
            .and_then(|mut file| file.write_all(&contents))
    };

    match result {
        Ok(()) => FS_FILE_OK,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => FS_FILE_EXISTS,
        Err(error) => {
            tracing::error!(%error, "failed to write downloaded file");
            FS_FILE_WRITEERROR
        }
    }
}

fn initialize_logging() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME is not set")?);
    let directory = home.join(if cfg!(target_os = "macos") {
        "Library/Logs/fast-sync"
    } else {
        ".local/state/fast-sync"
    });
    fs::create_dir_all(&directory)?;
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(directory.join("plugin.log"))?;
    tracing_subscriber::fmt()
        .compact()
        .with_ansi(false)
        .with_target(true)
        .with_file(true)
        .with_line_number(true)
        .with_writer(file)
        .try_init()
}

unsafe fn read_c_string(value: *const c_char) -> Option<String> {
    (!value.is_null()).then(|| {
        unsafe { CStr::from_ptr(value) }
            .to_string_lossy()
            .into_owned()
    })
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
