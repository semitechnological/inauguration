use std::path::Path;

use super::{DynamicModule, DynamicModuleError};

pub fn load_dynamic_module(_path: &Path) -> Result<Box<dyn DynamicModule>, DynamicModuleError> {
    Err(DynamicModuleError::UnsupportedPlatform)
}
