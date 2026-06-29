pub trait TypeChecker {
    fn check(&self, module: &mut hybrid_core::IrModule) -> Result<(), crate::CompileError>;
}

pub struct NullTypeChecker;

impl TypeChecker for NullTypeChecker {
    fn check(&self, _module: &mut hybrid_core::IrModule) -> Result<(), crate::CompileError> {
        // TODO: implement real type checker
        Ok(())
    }
}
