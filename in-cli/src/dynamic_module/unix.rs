use std::path::Path;

use libloading::Library;

use super::{
    DynamicModule, DynamicModuleError, InCallStatus, IN_MODULE_ENTRY_SYMBOL, ModuleDescriptor,
    validate_descriptor,
};

#[repr(C)]
struct RawModuleVTable {
    abi_version: u32,
    pointer_width: u32,
    endian: u32,
    layout_hash: u32,
    alloc: Option<unsafe extern "C" fn(u64, u64, u64) -> *mut ()>,
    dealloc: Option<unsafe extern "C" fn(*mut (), u64, u64, u64)>,
    init: Option<unsafe extern "C" fn(*const ()) -> InCallStatus>,
    shutdown: Option<unsafe extern "C" fn() -> InCallStatus>,
    symbol: Option<unsafe extern "C" fn(*const u8, u64) -> *const ()>,
    manifest: Option<unsafe extern "C" fn() -> *const ()>,
}

type VtableFn = unsafe extern "C" fn() -> *const RawModuleVTable;

pub struct UnixDynamicModule {
    _library: Library,
    vtable: *const RawModuleVTable,
}

impl DynamicModule for UnixDynamicModule {
    fn descriptor(&self) -> ModuleDescriptor {
        unsafe {
            ModuleDescriptor {
                abi_version: (*self.vtable).abi_version,
                pointer_width: (*self.vtable).pointer_width,
                endian: (*self.vtable).endian,
                layout_hash: (*self.vtable).layout_hash,
            }
        }
    }

    fn init(&self, host: *const ()) -> InCallStatus {
        unsafe {
            match (*self.vtable).init {
                Some(init) => init(host),
                None => InCallStatus::ok(),
            }
        }
    }

    fn shutdown(&self) -> InCallStatus {
        unsafe {
            match (*self.vtable).shutdown {
                Some(shutdown) => shutdown(),
                None => InCallStatus::ok(),
            }
        }
    }

    fn symbol(&self, name: &str) -> Option<*const ()> {
        unsafe {
            let lookup = (*self.vtable).symbol?;
            let ptr = lookup(name.as_ptr(), name.len() as u64);
            if ptr.is_null() {
                None
            } else {
                Some(ptr)
            }
        }
    }
}

pub fn load_dynamic_module(path: &Path) -> Result<Box<dyn DynamicModule>, DynamicModuleError> {
    unsafe {
        let library = Library::new(path).map_err(|err| DynamicModuleError::LoadFailed {
            path: path.display().to_string(),
            reason: err.to_string(),
        })?;
        let vtable_fn: libloading::Symbol<VtableFn> = library
            .get(IN_MODULE_ENTRY_SYMBOL.as_bytes())
            .map_err(|_| DynamicModuleError::EntryMissing {
                path: path.display().to_string(),
                symbol: IN_MODULE_ENTRY_SYMBOL.to_string(),
            })?;
        let vtable = vtable_fn();
        if vtable.is_null() {
            return Err(DynamicModuleError::LoadFailed {
                path: path.display().to_string(),
                reason: "entry returned null vtable".to_string(),
            });
        }
        let descriptor = ModuleDescriptor {
            abi_version: (*vtable).abi_version,
            pointer_width: (*vtable).pointer_width,
            endian: (*vtable).endian,
            layout_hash: (*vtable).layout_hash,
        };
        validate_descriptor(&descriptor)?;
        Ok(Box::new(UnixDynamicModule {
            _library: library,
            vtable,
        }))
    }
}