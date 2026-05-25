use std::cell::RefCell;
use std::path::Path;

thread_local! {
    static GUARD_STATE: RefCell<GuardState> = const { RefCell::new(GuardState::inactive()) };
}

#[derive(Debug, Clone)]
struct GuardState {
    active: bool,
    invocations: Vec<String>,
}

impl GuardState {
    const fn inactive() -> Self {
        Self {
            active: false,
            invocations: Vec::new(),
        }
    }
}

pub struct ExternalInvocationGuard;

impl ExternalInvocationGuard {
    pub fn enter() -> ExternalInvocationGuardToken {
        GUARD_STATE.with(|state| {
            let mut guard = state.borrow_mut();
            guard.active = true;
            guard.invocations.clear();
        });
        ExternalInvocationGuardToken
    }

    pub fn record(program: &str) {
        GUARD_STATE.with(|state| {
            let mut guard = state.borrow_mut();
            if guard.active {
                let basename = Path::new(program)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(program)
                    .to_string();
                guard.invocations.push(basename);
            }
        });
    }

    pub fn active_invocations() -> Vec<String> {
        GUARD_STATE.with(|state| state.borrow().invocations.clone())
    }

    pub fn is_active() -> bool {
        GUARD_STATE.with(|state| state.borrow().active)
    }
}

pub struct ExternalInvocationGuardToken;

impl Drop for ExternalInvocationGuardToken {
    fn drop(&mut self) {
        GUARD_STATE.with(|state| {
            state.borrow_mut().active = false;
        });
    }
}

pub const FORBIDDEN_OWNED_TOOLS: &[&str] = &[
    "swiftc", "rustc", "go", "cc", "clang", "tsc", "javac", "kotlinc", "ocamlc", "csc", "mcs",
    "python3", "python", "node", "zig", "v", "nim", "odin", "hare", "dart", "groovy", "ruby",
    "gcc", "g++", "ld", "link", "swift", "xcrun",
];

pub fn is_forbidden_owned_tool(program: &str) -> bool {
    let basename = Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(program);
    FORBIDDEN_OWNED_TOOLS
        .iter()
        .any(|tool| *tool == basename)
}

pub fn assert_no_forbidden_invocations(invocations: &[String]) -> Result<(), String> {
    let forbidden: Vec<String> = invocations
        .iter()
        .filter(|invocation| is_forbidden_owned_tool(invocation))
        .cloned()
        .collect();
    if forbidden.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "forbidden external tool invocation(s): {}",
            forbidden.join(", ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_records_only_when_active() {
        assert!(!ExternalInvocationGuard::is_active());
        ExternalInvocationGuard::record("/usr/bin/swiftc");
        assert!(ExternalInvocationGuard::active_invocations().is_empty());

        let _token = ExternalInvocationGuard::enter();
        assert!(ExternalInvocationGuard::is_active());
        ExternalInvocationGuard::record("/usr/bin/swiftc");
        ExternalInvocationGuard::record("/opt/homebrew/bin/clang");
        assert_eq!(
            ExternalInvocationGuard::active_invocations(),
            vec!["swiftc".to_string(), "clang".to_string()]
        );
        drop(_token);
        assert!(!ExternalInvocationGuard::is_active());
    }

    #[test]
    fn forbidden_tool_detection_uses_basename() {
        assert!(is_forbidden_owned_tool("/usr/bin/swiftc"));
        assert!(is_forbidden_owned_tool("rustc"));
        assert!(!is_forbidden_owned_tool("/usr/bin/in"));
    }

    #[test]
    fn assert_no_forbidden_invocations_fails_with_message() {
        let invocations = vec!["swiftc".to_string(), "in".to_string(), "clang".to_string()];
        let err = assert_no_forbidden_invocations(&invocations).expect_err("forbidden tools");
        assert!(err.contains("swiftc"));
        assert!(err.contains("clang"));
        assert!(assert_no_forbidden_invocations(&["in".to_string()]).is_ok());
    }
}
