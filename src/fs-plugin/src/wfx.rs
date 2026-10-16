//! Total Commander / Double Commander file system plugin (WFX) bindings.
//!
//! Reference: <https://ghisler.github.io/WFX-SDK/contents.htm>. Most functions come in
//! an ANSI and a wide (`…W`) flavour: <https://ghisler.github.io/WFX-SDK/unicode_support.htm>.
//!
//! Each export only decodes its arguments into a [`Command`], runs it through
//! [`filesystem::execute`] and encodes the result back into the ABI.

use crate::filesystem::{self, Command, Entry, Error, Reply};
use crate::remote_path::{DevicePath, RemotePath};
use std::ffi::{CStr, c_char, c_void};
use std::path::PathBuf;
use widestring::U16CStr;

/// <https://ghisler.github.io/WFX-SDK/fsinit.htm>
#[unsafe(no_mangle)]
pub extern "C" fn FsInit(
    _plugin_nr: i32,
    _progress_proc: ProgressProc<c_char>,
    _log_proc: LogProc<c_char>,
    _request_proc: RequestProc<c_char>,
) -> i32 {
    init()
}

/// <https://ghisler.github.io/WFX-SDK/fsinit.htm>
#[unsafe(no_mangle)]
pub extern "C" fn FsInitW(
    _plugin_nr: i32,
    _progress_proc: ProgressProc<u16>,
    _log_proc: LogProc<u16>,
    _request_proc: RequestProc<u16>,
) -> i32 {
    init()
}

/// <https://ghisler.github.io/WFX-SDK/fsfindfirst.htm>
#[unsafe(no_mangle)]
pub unsafe extern "C" fn FsFindFirst(path: *const c_char, data: *mut FindDataA) -> *mut c_void {
    let path = ansi(path);
    let encode = |e: &Entry| FindData::new(e, e.name.bytes());
    unsafe { find_first(path, data, encode) }
}

/// <https://ghisler.github.io/WFX-SDK/fsfindfirst.htm>
#[unsafe(no_mangle)]
pub unsafe extern "C" fn FsFindFirstW(path: *const u16, data: *mut FindDataW) -> *mut c_void {
    let path = wide(path);
    let encode = |e: &Entry| FindData::new(e, e.name.encode_utf16());
    unsafe { find_first(path, data, encode) }
}

/// <https://ghisler.github.io/WFX-SDK/fsfindnext.htm>
#[unsafe(no_mangle)]
pub unsafe extern "C" fn FsFindNext(handle: *mut c_void, data: *mut FindDataA) -> i32 {
    let encode = |e: &Entry| FindData::new(e, e.name.bytes());
    unsafe { find_next(handle, data, encode) }
}

/// <https://ghisler.github.io/WFX-SDK/fsfindnext.htm>
#[unsafe(no_mangle)]
pub unsafe extern "C" fn FsFindNextW(handle: *mut c_void, data: *mut FindDataW) -> i32 {
    let encode = |e: &Entry| FindData::new(e, e.name.encode_utf16());
    unsafe { find_next(handle, data, encode) }
}

/// <https://ghisler.github.io/WFX-SDK/fsfindclose.htm>
#[unsafe(no_mangle)]
pub unsafe extern "C" fn FsFindClose(handle: *mut c_void) -> i32 {
    if !handle.is_null() && handle != INVALID_HANDLE_VALUE {
        let handle = handle.cast::<FindHandle>();
        drop(unsafe { Box::from_raw(handle) });
    }
    0
}

/// <https://ghisler.github.io/WFX-SDK/fsgetfile.htm>
#[unsafe(no_mangle)]
pub unsafe extern "C" fn FsGetFile(
    remote_name: *const c_char,
    local_name: *const c_char,
    copy_flags: i32,
    _remote_info: *mut c_void,
) -> i32 {
    get_file(ansi(remote_name), ansi(local_name), copy_flags)
}

/// <https://ghisler.github.io/WFX-SDK/fsgetfile.htm>
#[unsafe(no_mangle)]
pub unsafe extern "C" fn FsGetFileW(
    remote_name: *const u16,
    local_name: *const u16,
    copy_flags: i32,
    _remote_info: *mut c_void,
) -> i32 {
    get_file(wide(remote_name), wide(local_name), copy_flags)
}

/// <https://ghisler.github.io/WFX-SDK/fsgetdefrootname.htm>
#[unsafe(no_mangle)]
pub unsafe extern "C" fn FsGetDefRootName(root_name: *mut c_char, max_len: i32) {
    let Ok(max_len @ 1..) = usize::try_from(max_len) else {
        return;
    };
    if root_name.is_null() {
        return;
    }
    let name = filesystem::ROOT_NAME.as_bytes();
    let len = name.len().min(max_len - 1);
    let source = name.as_ptr().cast();
    unsafe {
        root_name.copy_from_nonoverlapping(source, len);
        let terminator = root_name.add(len);
        terminator.write(0);
    };
}

fn init() -> i32 {
    match filesystem::execute(Command::Init) {
        Ok(_) => 0,
        Err(_) => 1,
    }
}

type FindHandle = std::vec::IntoIter<Entry>;

/// Lists a directory, writes its first entry, and returns a handle owning the remaining entries.
unsafe fn find_first<C>(
    path: Option<String>,
    data: *mut FindData<C>,
    encode: impl Fn(&Entry) -> FindData<C>,
) -> *mut c_void {
    let Some(path) = path.filter(|_| !data.is_null()) else {
        tracing::error!("null WFX listing path or output buffer");
        return INVALID_HANDLE_VALUE;
    };
    let path = match path.parse::<RemotePath>() {
        Ok(path) => path,
        Err(error) => {
            tracing::error!(%error, "invalid WFX listing path");
            return INVALID_HANDLE_VALUE;
        }
    };
    let mut entries = match filesystem::execute(Command::List { path }) {
        Ok(Reply::Entries(entries)) => entries.into_iter(),
        Ok(Reply::Done) => {
            tracing::error!("list command returned no entries reply");
            return INVALID_HANDLE_VALUE;
        }
        Err(_) => return INVALID_HANDLE_VALUE,
    };
    let Some(first) = entries.next() else {
        return INVALID_HANDLE_VALUE;
    };
    let first = encode(&first);
    unsafe { data.write(first) };
    Box::into_raw(Box::new(entries)).cast()
}

/// Advances the handle's iterator and writes one entry; returns 0 when exhausted or invalid.
unsafe fn find_next<C>(
    handle: *mut c_void,
    data: *mut FindData<C>,
    encode: impl Fn(&Entry) -> FindData<C>,
) -> i32 {
    if handle.is_null() || handle == INVALID_HANDLE_VALUE || data.is_null() {
        tracing::error!("invalid WFX listing handle or output buffer");
        return 0;
    }
    let handle = handle.cast::<FindHandle>();
    let entries = unsafe { &mut *handle };
    let Some(entry) = entries.next() else {
        return 0;
    };
    let entry = encode(&entry);
    unsafe { data.write(entry) };
    1
}

fn get_file(remote: Option<String>, local: Option<String>, copy_flags: i32) -> i32 {
    let (Some(remote), Some(local)) = (remote, local) else {
        tracing::error!("null WFX remote or local download path");
        return FS_FILE_NOTFOUND;
    };
    let remote = match DevicePath::parse_file(&remote) {
        Ok(remote) => remote,
        Err(error) => {
            tracing::error!(%error, "invalid WFX download path");
            return FS_FILE_NOTFOUND;
        }
    };
    let overwrite = copy_flags & FS_COPYFLAGS_OVERWRITE != 0;
    let local = PathBuf::from(local);
    match filesystem::execute(Command::Get {
        remote,
        local,
        overwrite,
    }) {
        Ok(_) => FS_FILE_OK,
        Err(Error::NotFound) => FS_FILE_NOTFOUND,
        Err(Error::Exists) => FS_FILE_EXISTS,
        Err(Error::Write(_)) => FS_FILE_WRITEERROR,
        Err(_) => FS_FILE_READERROR,
    }
}

// SAFETY: only called with host-provided pointers, which the SDK guarantees to be
// null or null-terminated strings.
fn ansi(value: *const c_char) -> Option<String> {
    (!value.is_null()).then(|| {
        unsafe { CStr::from_ptr(value) }
            .to_string_lossy()
            .into_owned()
    })
}

// SAFETY: see `ansi`.
fn wide(value: *const u16) -> Option<String> {
    (!value.is_null()).then(|| unsafe { U16CStr::from_ptr_str(value) }.to_string_lossy())
}

/// <https://ghisler.github.io/WFX-SDK/progressproc.htm>
type ProgressProc<C> = Option<unsafe extern "C" fn(i32, *mut C, *mut C, i32) -> i32>;
/// <https://ghisler.github.io/WFX-SDK/logproc.htm>
type LogProc<C> = Option<unsafe extern "C" fn(i32, i32, *mut C)>;
/// <https://ghisler.github.io/WFX-SDK/requestproc.htm>
type RequestProc<C> = Option<unsafe extern "C" fn(i32, i32, *mut C, *mut C, *mut C, i32) -> i32>;

const INVALID_HANDLE_VALUE: *mut c_void = usize::MAX as *mut c_void;

// Flags and return codes: <https://ghisler.github.io/WFX-SDK/fsgetfile.htm>
const FS_COPYFLAGS_OVERWRITE: i32 = 1;
const FS_FILE_OK: i32 = 0;
const FS_FILE_EXISTS: i32 = 1;
const FS_FILE_NOTFOUND: i32 = 2;
const FS_FILE_READERROR: i32 = 3;
const FS_FILE_WRITEERROR: i32 = 4;

// Windows SDK constant used by `WIN32_FIND_DATA`.
const MAX_PATH: usize = 260;
const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
const FILE_ATTRIBUTE_NORMAL: u32 = 0x80;

/// `WIN32_FIND_DATA`: <https://ghisler.github.io/WFX-SDK/win32_find_data.htm>
#[repr(C, packed)]
pub struct FindData<C> {
    attributes: u32,
    /// `FILETIME`s as `[low, high]`: creation, last access, last write.
    times: [[u32; 2]; 3],
    size_high: u32,
    size_low: u32,
    reserved: [u32; 2],
    file_name: [C; MAX_PATH],
    alternate_file_name: [C; 14],
}

pub type FindDataA = FindData<u8>;
pub type FindDataW = FindData<u16>;

const _: () = assert!(size_of::<FindDataA>() == 318 && size_of::<FindDataW>() == 592);

impl<C: Copy + Default> FindData<C> {
    fn new(entry: &Entry, name: impl Iterator<Item = C>) -> Self {
        let (attributes, size) = match entry.is_directory {
            true => (FILE_ATTRIBUTE_DIRECTORY, 0),
            false => (FILE_ATTRIBUTE_NORMAL, entry.size),
        };
        let mut file_name = [C::default(); MAX_PATH];
        for (dst, c) in file_name.iter_mut().take(MAX_PATH - 1).zip(name) {
            *dst = c;
        }
        Self {
            attributes,
            times: [[0; 2]; 3],
            size_high: (size >> 32) as u32,
            size_low: size as u32,
            reserved: [0; 2],
            file_name,
            alternate_file_name: [C::default(); 14],
        }
    }
}
