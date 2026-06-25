//! Compilation scheduler — separates WHAT from HOW.
//!
//! Frontend submits function-level compile jobs. Scheduler owns
//! execution strategy (CPU parallel, GPU, hybrid). Backends
//! implement CompileBackend.

use std::sync::Arc;

use crate::core_ir::{Decl, UnifiedModule};

/// A single compile job: lower one function.
#[derive(Clone, Debug)]
pub struct CompileJob {
    pub func_name: String,
    pub func_decl: Decl,
    pub module: Arc<UnifiedModule>,
    pub entry_name: Option<String>,
}

impl CompileJob {
    pub fn from_module(module: &Arc<UnifiedModule>, entry: &str) -> Vec<Self> {
        let mut jobs = Vec::new();
        for decl in &module.decls {
            if let Decl::Function { name, .. } = decl {
                jobs.push(CompileJob {
                    func_name: name.clone(),
                    func_decl: decl.clone(),
                    module: Arc::clone(module),
                    entry_name: Some(entry.to_string()),
                });
            }
        }
        jobs
    }
}

/// Abstract backend for batch compilation.
pub trait CompileBackend: Send + Sync {
    fn compile_batch(&self, jobs: &[CompileJob]) -> Result<Vec<(String, u32, Vec<u8>)>, String>;
}

/// CPU backend — lowers functions sequentially.
pub struct CpuBackend;

impl CompileBackend for CpuBackend {
    fn compile_batch(&self, jobs: &[CompileJob]) -> Result<Vec<(String, u32, Vec<u8>)>, String> {
        let mut results = Vec::with_capacity(jobs.len());
        for job in jobs {
            // ponytail: sequential for now; parallel via rayon later
            let code = lower_one(&job.func_decl, &job.module)?;
            results.push((job.func_name.clone(), 0u32, code));
        }
        // Compute offsets serially
        let mut off = 0u32;
        for r in &mut results {
            r.1 = off;
            off += r.2.len() as u32;
        }
        Ok(results)
    }
}

fn lower_one(decl: &Decl, _module: &UnifiedModule) -> Result<Vec<u8>, String> {
    let Decl::Function { name, .. } = decl else {
        return Err("not a function".to_string());
    };
    // ponytail: stub — actual lowering uses compile_jit pipeline
    // For now, returns empty code block. Real implementation calls
    // native_emit::lower machinery.
    Err(format!(
        "lower_one not wired — use compile_jit for `{name}`"
    ))
}

/// GPU backend — architecture placeholder.
pub struct GpuBackend;

impl CompileBackend for GpuBackend {
    fn compile_batch(&self, _jobs: &[CompileJob]) -> Result<Vec<(String, u32, Vec<u8>)>, String> {
        Err("GPU backend not implemented".to_string())
    }
}

/// Scheduler — owns backend and dispatches jobs.
pub struct CompileScheduler {
    backend: Box<dyn CompileBackend>,
}

impl CompileScheduler {
    pub fn new_cpu() -> Self {
        Self {
            backend: Box::new(CpuBackend),
        }
    }
    pub fn set_backend(&mut self, backend: Box<dyn CompileBackend>) {
        self.backend = backend;
    }
    pub fn compile(&self, jobs: &[CompileJob]) -> Result<Vec<(String, u32, Vec<u8>)>, String> {
        self.backend.compile_batch(jobs)
    }
}

impl Default for CompileScheduler {
    fn default() -> Self {
        Self::new_cpu()
    }
}
