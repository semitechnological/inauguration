use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InArenaHandle {
    pub id: u64,
    pub generation: u64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InBufU8 {
    pub ptr: *mut u8,
    pub len: u64,
    pub cap: u64,
    pub allocator_id: u64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InSliceU8 {
    pub ptr: *const u8,
    pub len: u64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InBorrowToken {
    pub arena_id: u64,
    pub generation: u64,
    pub start: u64,
    pub len: u64,
    pub flags: u64,
}

struct ArenaState {
    generation: u64,
    storage: Vec<u8>,
    cursor: usize,
}

static NEXT_ARENA_ID: AtomicU64 = AtomicU64::new(1);
static ARENAS: OnceLock<Mutex<HashMap<u64, ArenaState>>> = OnceLock::new();

fn arenas() -> &'static Mutex<HashMap<u64, ArenaState>> {
    ARENAS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn in_arena_create(reserve_bytes: u64, _flags: u32) -> InArenaHandle {
    let id = NEXT_ARENA_ID.fetch_add(1, Ordering::SeqCst);
    let reserve = reserve_bytes.max(64) as usize;
    let state = ArenaState {
        generation: 1,
        storage: vec![0; reserve],
        cursor: 0,
    };
    arenas().lock().expect("arena map lock").insert(id, state);
    InArenaHandle { id, generation: 1 }
}

pub fn in_arena_reset(arena: InArenaHandle) {
    let mut map = arenas().lock().expect("arena map lock");
    if let Some(state) = map.get_mut(&arena.id) {
        if state.generation != arena.generation {
            return;
        }
        state.cursor = 0;
    }
}

pub fn in_arena_destroy(arena: InArenaHandle) {
    let mut map = arenas().lock().expect("arena map lock");
    if let Some(state) = map.get(&arena.id)
        && state.generation == arena.generation
    {
        map.remove(&arena.id);
    }
}

pub fn in_buf_from_host_arena(arena: InArenaHandle, len: u64, align: u64) -> InBufU8 {
    let align = align.max(1) as usize;
    let len = len as usize;
    let mut map = arenas().lock().expect("arena map lock");
    let Some(state) = map.get_mut(&arena.id) else {
        return InBufU8 {
            ptr: std::ptr::null_mut(),
            len: 0,
            cap: 0,
            allocator_id: arena.id,
        };
    };
    if state.generation != arena.generation {
        return InBufU8 {
            ptr: std::ptr::null_mut(),
            len: 0,
            cap: 0,
            allocator_id: arena.id,
        };
    }
    let aligned = align_up(state.cursor, align);
    let end = aligned.saturating_add(len);
    if end > state.storage.len() {
        state.storage.resize(end, 0);
    }
    state.cursor = end;
    let ptr = unsafe { state.storage.as_mut_ptr().add(aligned) };
    InBufU8 {
        ptr,
        len: len as u64,
        cap: len as u64,
        allocator_id: arena.id,
    }
}

pub fn in_borrow_bytes(buf: &mut InBufU8, out_token: &mut InBorrowToken) -> InSliceU8 {
    out_token.arena_id = buf.allocator_id;
    out_token.generation = 0;
    out_token.start = 0;
    out_token.len = buf.len;
    out_token.flags = 0;
    InSliceU8 {
        ptr: buf.ptr,
        len: buf.len,
    }
}

pub fn in_borrow_validate(token: InBorrowToken) -> u32 {
    let map = arenas().lock().expect("arena map lock");
    let Some(state) = map.get(&token.arena_id) else {
        return 2;
    };
    if token.len == 0 {
        return 1;
    }
    if token.start.saturating_add(token.len) > state.storage.len() as u64 {
        return 2;
    }
    0
}

fn align_up(value: usize, align: usize) -> usize {
    let mask = align.saturating_sub(1);
    (value + mask) & !mask
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arena_alloc_and_reset() {
        let arena = in_arena_create(128, 0);
        let buf = in_buf_from_host_arena(arena, 16, 8);
        assert!(!buf.ptr.is_null());
        assert_eq!(buf.len, 16);
        in_arena_reset(arena);
        let buf2 = in_buf_from_host_arena(arena, 8, 8);
        assert!(!buf2.ptr.is_null());
        in_arena_destroy(arena);
    }

    #[test]
    fn borrow_token_validates_live_slice() {
        let arena = in_arena_create(64, 0);
        let mut buf = in_buf_from_host_arena(arena, 8, 8);
        let mut token = InBorrowToken {
            arena_id: 0,
            generation: 0,
            start: 0,
            len: 0,
            flags: 0,
        };
        let slice = in_borrow_bytes(&mut buf, &mut token);
        assert_eq!(slice.len, 8);
        assert_eq!(in_borrow_validate(token), 0);
        in_arena_destroy(arena);
        assert_eq!(in_borrow_validate(token), 2);
    }
}
