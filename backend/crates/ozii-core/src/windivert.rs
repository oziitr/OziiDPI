//! Minimal runtime-loaded FFI bindings for the WinDivert user-mode library,
//! mirroring windivert.h exactly for WinDivert 2.2.x.
//!
//! WinDivert.dll is LGPLv3; it is loaded dynamically at runtime (never linked
//! statically). The driver is auto-(re)started by the library on handle open
//! when running elevated, matching production WinDivert 2.2.x behavior.

use std::ffi::{c_char, c_void};
use std::os::windows::io::RawHandle;
use std::path::Path;
use std::sync::Arc;

use winapi::shared::minwindef::HMODULE;
use winapi::um::libloaderapi::{FreeLibrary, GetModuleHandleW, GetProcAddress};
use winapi::um::libloaderapi::{LoadLibraryExW, LOAD_WITH_ALTERED_SEARCH_PATH};

pub const WINDIVERT_LAYER_NETWORK: u32 = 0;

pub const WINDIVERT_FLAG_SNIFF: u64 = 0x0001;
pub const WINDIVERT_FLAG_DROP: u64 = 0x0002;
pub const WINDIVERT_FLAG_RECV_ONLY: u64 = 0x0004;
pub const WINDIVERT_FLAG_SEND_ONLY: u64 = 0x0008;
pub const WINDIVERT_FLAG_NO_INSTALL: u64 = 0x0010;

pub const WINDIVERT_SHUTDOWN_RECV: u32 = 0x1;
pub const WINDIVERT_SHUTDOWN_SEND: u32 = 0x2;
pub const WINDIVERT_SHUTDOWN_BOTH: u32 = 0x3;

// Bitfield layout of `WinDivertAddress.flags` (see windivert.h):
// Layer:8, Event:8, Sniffed:1, Outbound:1, Loopback:1, Impostor:1, IPv6:1,
// IPChecksum:1, TCPChecksum:1, UDPChecksum:1, Reserved1:8.
const ADDR_BIT_SNIFFED: u32 = 1 << 16;
const ADDR_BIT_OUTBOUND: u32 = 1 << 17;
const ADDR_BIT_LOOPBACK: u32 = 1 << 18;
const ADDR_BIT_IMPOSTOR: u32 = 1 << 19;
const ADDR_BIT_IPV6: u32 = 1 << 20;
const ADDR_BIT_IP_CHECKSUM: u32 = 1 << 21;
const ADDR_BIT_TCP_CHECKSUM: u32 = 1 << 22;
const ADDR_BIT_UDP_CHECKSUM: u32 = 1 << 23;

const ADDR_UNION_SIZE: usize = 64;

/// Equivalent of WINDIVERT_ADDRESS:
///   INT64 Timestamp;                          (offset 0, 8 bytes)
///   UINT32 <bitfield Layer:8 Event:8 ...>;    (offset 8, 4 bytes)
///   UINT32 Reserved2;                         (offset 12, 4 bytes)
///   union { WINDIVERT_DATA_NETWORK {...}; ... } (offset 16, 64 bytes)
/// Total 80 bytes — WinDivertRecv fills the whole structure.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct WinDivertAddress {
    pub timestamp: i64,
    pub flags: u32,
    pub reserved2: u32,
    pub union_data: [u8; ADDR_UNION_SIZE],
}

impl WinDivertAddress {
    pub fn zeroed() -> Self {
        Self {
            timestamp: 0,
            flags: 0,
            reserved2: 0,
            union_data: [0u8; ADDR_UNION_SIZE],
        }
    }

    #[inline]
    pub fn outbound(&self) -> bool {
        self.flags & ADDR_BIT_OUTBOUND != 0
    }

    #[inline]
    pub fn set_outbound(&mut self, value: bool) {
        set_bit(&mut self.flags, ADDR_BIT_OUTBOUND, value);
    }

    #[inline]
    pub fn loopback(&self) -> bool {
        self.flags & ADDR_BIT_LOOPBACK != 0
    }

    #[inline]
    pub fn impostor(&self) -> bool {
        self.flags & ADDR_BIT_IMPOSTOR != 0
    }

    #[inline]
    pub fn set_impostor(&mut self, value: bool) {
        set_bit(&mut self.flags, ADDR_BIT_IMPOSTOR, value);
    }

    #[inline]
    pub fn ipv6(&self) -> bool {
        self.flags & ADDR_BIT_IPV6 != 0
    }

    #[inline]
    pub fn ip_checksum(&self) -> bool {
        self.flags & ADDR_BIT_IP_CHECKSUM != 0
    }

    #[inline]
    pub fn tcp_checksum(&self) -> bool {
        self.flags & ADDR_BIT_TCP_CHECKSUM != 0
    }
}

#[inline]
fn set_bit(word: &mut u32, mask: u32, value: bool) {
    if value {
        *word |= mask;
    } else {
        *word &= !mask;
    }
}

pub type XcOpen = unsafe extern "system" fn(*const c_char, u32, i16, u64) -> RawHandle;
pub type XcRecv =
    unsafe extern "system" fn(RawHandle, *mut c_void, u32, *mut WinDivertAddress) -> i32;
pub type XcSend =
    unsafe extern "system" fn(RawHandle, *const c_void, u32, *const WinDivertAddress) -> i32;
pub type XcShutdown = unsafe extern "system" fn(RawHandle, u32) -> i32;
pub type XcClose = unsafe extern "system" fn(RawHandle) -> i32;

/// Keeps the DLL loaded while the API pointers are valid.
struct DllGuard(HMODULE);

// Raw module handles are safe to share across threads; the OS keeps the
// module mapped until the last reference is dropped.
unsafe impl Send for DllGuard {}
unsafe impl Sync for DllGuard {}

impl Drop for DllGuard {
    fn drop(&mut self) {
        unsafe {
            FreeLibrary(self.0);
        }
    }
}

pub struct WinDivertApi {
    _guard: Arc<DllGuard>,
    pub open: XcOpen,
    pub recv: XcRecv,
    pub send: XcSend,
    pub shutdown: XcShutdown,
    pub close: XcClose,
}

// WinDivertApi only holds stateless C function pointers and the module
// guard; passing it between threads is safe.
unsafe impl Send for WinDivertApi {}
unsafe impl Sync for WinDivertApi {}

impl WinDivertApi {
    /// Tries `dir/WinDivert.dll` first (so a bundled copy next to the
    /// executable wins), then the regular DLL search paths.
    pub fn load(dir: Option<&Path>) -> Result<Self, String> {
        if let Some(d) = dir {
            let candidate = d.join("WinDivert.dll");
            if candidate.exists() {
                if let Ok(lib) = RawDll::from_path(&candidate) {
                    return lib.finish();
                }
            }
        }
        // Loading by name relies on PATH / exe directory lookup.
        let name = "WinDivert.dll";
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        let h = unsafe {
            LoadLibraryExW(
                wide.as_ptr(),
                std::ptr::null_mut(),
                LOAD_WITH_ALTERED_SEARCH_PATH as u32,
            )
        };
        if h.is_null() {
            // Last chance: probe the module table (maybe already loaded).
            let h2 = unsafe { GetModuleHandleW(wide.as_ptr()) };
            if h2.is_null() {
                return Err(format!(
                    "WinDivert.dll could not be loaded (GetLastError={})",
                    unsafe { winapi::um::errhandlingapi::GetLastError() }
                ));
            }
            return unsafe { RawDll::with_handle(h2).finish() };
        }
        unsafe { RawDll::with_handle(h).finish() }
    }
}

struct RawDll {
    handle: HMODULE,
    owned: bool,
}

impl RawDll {
    fn from_path(path: &Path) -> Result<Self, String> {
        use std::os::windows::ffi::OsStrExt;
        let wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let h = unsafe {
            LoadLibraryExW(
                wide.as_ptr(),
                std::ptr::null_mut(),
                LOAD_WITH_ALTERED_SEARCH_PATH as u32,
            )
        };
        if h.is_null() {
            return Err(format!(
                "LoadLibraryExW failed for {} (GetLastError={})",
                path.display(),
                unsafe { winapi::um::errhandlingapi::GetLastError() }
            ));
        }
        Ok(Self { handle: h, owned: true })
    }

    unsafe fn with_handle(h: HMODULE) -> Self {
        Self {
            handle: h,
            owned: false,
        }
    }

    fn proc(&self, name: &str) -> Result<usize, String> {
        use std::ffi::CString;
        let cname = CString::new(name).map_err(|e| e.to_string())?;
        let addr = unsafe { GetProcAddress(self.handle, cname.as_ptr()) };
        if addr.is_null() {
            return Err(format!("export not found: {name}"));
        }
        Ok(addr as usize)
    }

    fn finish(self) -> Result<WinDivertApi, String> {
        let raw = self;
        let guard = Arc::new(DllGuard(raw.handle));
        unsafe {
            Ok(WinDivertApi {
                _guard: guard,
                open: std::mem::transmute::<usize, XcOpen>(raw.proc("WinDivertOpen")?),
                recv: std::mem::transmute::<usize, XcRecv>(raw.proc("WinDivertRecv")?),
                send: std::mem::transmute::<usize, XcSend>(raw.proc("WinDivertSend")?),
                shutdown: std::mem::transmute::<usize, XcShutdown>(
                    raw.proc("WinDivertShutdown")?,
                ),
                close: std::mem::transmute::<usize, XcClose>(raw.proc("WinDivertClose")?),
            })
        }
    }
}

/// Small helper: asserts the compiled address layout matches the C header.
pub fn address_layout_sane() -> bool {
    std::mem::size_of::<WinDivertAddress>() == 80
}
