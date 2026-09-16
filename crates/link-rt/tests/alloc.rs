//! The allocation entry points, exercised through the C ABI.

use link_rt::*;

#[test]
fn alloc_returns_writable_aligned_memory() {
    unsafe {
        let p = link_alloc(64, 8);
        assert!(!p.is_null());
        assert_eq!(p as usize % 8, 0);
        std::ptr::write_bytes(p, 0xAB, 64);
        assert_eq!(*p, 0xAB);
        assert_eq!(*p.add(63), 0xAB);
        link_dealloc(p, 64, 8);
    }
}

#[test]
fn alloc_honours_large_alignment() {
    unsafe {
        for align in [1usize, 2, 4, 8, 16, 32, 64, 128] {
            let p = link_alloc(align * 3, align);
            assert!(!p.is_null());
            assert_eq!(p as usize % align, 0, "alignment {align} not honoured");
            link_dealloc(p, align * 3, align);
        }
    }
}

#[test]
fn zero_sized_alloc_is_dangling_but_usable() {
    unsafe {
        let p = link_alloc(0, 16);
        assert!(!p.is_null(), "a zero-sized allocation is never null");
        assert_eq!(p as usize % 16, 0);
        // Copying zero bytes to or from it is well defined.
        std::ptr::copy_nonoverlapping(p as *const u8, p, 0);
        // And freeing it is a no-op, not a call into the system allocator.
        link_dealloc(p, 0, 16);
    }
}

#[test]
fn realloc_grows_and_preserves_contents() {
    unsafe {
        let p = link_alloc(16, 8);
        for i in 0..16u8 {
            *p.add(i as usize) = i;
        }
        let p = link_realloc(p, 16, 4096, 8);
        assert_eq!(p as usize % 8, 0);
        for i in 0..16u8 {
            assert_eq!(*p.add(i as usize), i, "byte {i} lost across realloc");
        }
        link_dealloc(p, 4096, 8);
    }
}

#[test]
fn realloc_shrinks_and_preserves_the_prefix() {
    unsafe {
        let p = link_alloc(256, 8);
        for i in 0..256usize {
            *p.add(i) = (i % 251) as u8;
        }
        let p = link_realloc(p, 256, 8, 8);
        for i in 0..8usize {
            assert_eq!(*p.add(i), (i % 251) as u8);
        }
        link_dealloc(p, 8, 8);
    }
}

#[test]
fn realloc_from_zero_allocates() {
    unsafe {
        let p = link_alloc(0, 8);
        let p = link_realloc(p, 0, 32, 8);
        assert!(!p.is_null());
        std::ptr::write_bytes(p, 7, 32);
        assert_eq!(*p.add(31), 7);
        link_dealloc(p, 32, 8);
    }
}

#[test]
fn realloc_to_zero_frees_and_returns_dangling() {
    unsafe {
        let p = link_alloc(32, 8);
        let p = link_realloc(p, 32, 0, 8);
        assert!(!p.is_null());
        assert_eq!(p as usize % 8, 0);
        link_dealloc(p, 0, 8);
    }
}

#[test]
fn realloc_to_the_same_size_is_valid() {
    unsafe {
        let p = link_alloc(32, 8);
        *p = 9;
        let p = link_realloc(p, 32, 32, 8);
        assert_eq!(*p, 9);
        link_dealloc(p, 32, 8);
    }
}

#[test]
fn realloc_preserves_over_alignment() {
    unsafe {
        let mut size = 64usize;
        let mut p = link_alloc(size, 64);
        assert_eq!(p as usize % 64, 0);
        *p = 0x5A;
        for new_size in [128usize, 256, 1024, 4096, 64, 4096] {
            p = link_realloc(p, size, new_size, 64);
            size = new_size;
            assert_eq!(p as usize % 64, 0, "alignment lost resizing to {size}");
            assert_eq!(*p, 0x5A, "contents lost resizing to {size}");
        }
        link_dealloc(p, size, 64);
    }
}
