use core::{
    alloc::{AllocError, Allocator, GlobalAlloc, Layout},
    ptr::NonNull,
};

use crate::HEAP_ALLOCATOR;
use crate::{print, utils::safe::Safe};

pub const HEAP_START: usize = 0o_000_020_000_000_0000;
pub const HEAP_SIZE: usize = 16 * 1024 * 1024; // 16 MiB

struct Block {
    next: Option<&'static mut Block>,
    used: bool,
    size: usize,
    address: usize,
}

impl Block {
    const fn new(address: usize, size: usize) -> Block {
        Block {
            next: None,
            used: false,
            size: size,
            address: address,
        }
    }
}

pub struct LinkedListHeap {
    heap_start: usize,
    heap_end: usize,
    size: usize,

    /// this is not the head reference,
    /// this block's .next is the actual
    /// head, rust disallows mutable references
    /// in cosnt functions.
    head: Block,
}

impl LinkedListHeap {
    pub const fn empty() -> Self {
        let head = Block::new(0, 0);
        Self {
            heap_start: 0,
            heap_end: 0,
            size: 0,
            head: head,
        }
    }

    fn init(&mut self, heap_start: usize, heap_end: usize) {
        let heap_size = heap_end - heap_start;
        // let mut start_block = Block::new(heap_start, heap_size);
        // let start_block_ptr = get_block_ptr(&mut start_block);
        let start_block = match create_block(heap_start, heap_size) {
            Some(b) => b,
            None => unreachable!(),
        };

        self.heap_start = heap_start;
        self.heap_end = heap_end;
        self.size = heap_size;

        self.head.next = Some(start_block);
    }

    fn allocate_internal(&mut self, size: usize, _align: usize) -> Result<*mut u8, AllocError> {
        let mut head = &mut self.head;
        loop {
            let Some(ref mut block) = head.next else {
                break;
            };

            if !block.used && block.size == size {
                block.used = true;

                let addr = block.address + core::mem::size_of::<Block>();
                return Ok(addr as *mut u8);
            }

            if !block.used && block.size > size {
                // found block, split up
                let remainder_size = block.size - size;
                let remainder_addr = block.address + size;

                let remainder_next = match create_block(remainder_addr, remainder_size) {
                    Some(b) => b,
                    None => {
                        // this previously threw an allocator error...
                        // idk why it was done that way, but just switching
                        // to the next block seems logical to me
                        head = head.next.as_mut().unwrap();
                        continue;
                    }
                };

                remainder_next.next = block.next.take();
                block.next = Some(remainder_next);

                block.used = true;
                block.size = size;

                let addr = block.address + core::mem::size_of::<Block>();
                return Ok(addr as *mut u8);
            }

            head = head.next.as_mut().unwrap();
        }

        return Err(AllocError);
    }

    fn dealloc_internal(&mut self, addr: usize, _size: usize) {
        let block_size = core::mem::size_of::<Block>();
        let block_addr = addr - block_size;

        let block_ptr = block_addr as *mut Block;
        let block = unsafe { &mut *block_ptr };

        block.used = false;
        self.merge_right(block);

        // TODO: Maybe we should also merge left?

        // move head to current free block
        if block.address >= self.head.address {
            return;
        }

        self.head.next = Some(block);
    }

    /// Merges a free block with the next block if it's free
    fn merge_right(&mut self, block: &mut Block) {
        let Some(ref mut next) = block.next else {
            return;
        };

        if next.used {
            return;
        }

        // merge
        let new_size = block.size + next.size;
        block.size = new_size;

        // set new next
        let Some(ref mut new_next) = next.next else {
            block.next = None;
            return;
        };

        let new_next_ptr = new_next.address as *mut Block;
        let new_next_ref = unsafe { &mut *new_next_ptr };
        block.next = Some(new_next_ref);
    }

    /// Aligns a layout such that that the allocated memory region
    /// is also capable of holding the block structure.
    fn block_align_size(&self, layout: Layout) -> (usize, usize) {
        let layout = layout
            .align_to(core::mem::align_of::<Block>())
            .expect("Couldn't align block")
            .pad_to_align();

        let size = layout.size() + core::mem::size_of::<Block>();
        (size, layout.align())
    }
}

unsafe impl<'a> Allocator for Safe<LinkedListHeap> {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let mut allocator = self.lock();

        let (size, align) = allocator.block_align_size(layout);
        let block = allocator.allocate_internal(size, align);

        if let Err(_) = block {
            return Err(AllocError);
        }

        let block = block.unwrap();
        let start_ptr = NonNull::<u8>::new(block).unwrap();
        let slice = NonNull::slice_from_raw_parts(start_ptr, layout.size());
        Ok(slice)
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        let mut allocator = self.lock();
        allocator.dealloc_internal(ptr.addr().get(), layout.size());
    }
}

unsafe impl GlobalAlloc for Safe<LinkedListHeap> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut allocator = self.lock();

        let (size, align) = allocator.block_align_size(layout);
        let block = allocator.allocate_internal(size, align);

        match block {
            Ok(ptr) => ptr,
            Err(_) => core::ptr::null_mut(),
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let mut allocator = self.lock();
        allocator.dealloc_internal(ptr as usize, layout.size());
    }
}

fn create_block(addr: usize, size: usize) -> Option<&'static mut Block> {
    let is_correct_align = align_up(addr, core::mem::align_of::<Block>()) == addr;
    let can_hold_structure = size >= core::mem::size_of::<Block>();

    if !is_correct_align || !can_hold_structure {
        return None;
    }

    // something fishy is happening with the
    // pointers, when reading out of the
    // reference, the addr is suddenly zero

    let block = Block::new(addr, size);

    let block_ptr = addr as *mut Block;
    unsafe { block_ptr.write(block) };

    let reference = unsafe { &mut *block_ptr };
    Some(reference)
}

/// Align downwards. Returns the greatest x with alignment `align`
/// so that x <= addr. The alignment must be a power of 2.
pub fn align_down(addr: usize, align: usize) -> usize {
    if align.is_power_of_two() {
        addr & !(align - 1)
    } else if align == 0 {
        addr
    } else {
        panic!("`align` must be a power of 2");
    }
}

/// Align upwards. Returns the smallest x with alignment `align`
/// so that x >= addr. The alignment must be a power of 2.
pub fn align_up(addr: usize, align: usize) -> usize {
    align_down(addr + align - 1, align)
}

pub unsafe fn init_heap() {
    let mut allocator = HEAP_ALLOCATOR.lock();
    allocator.init(HEAP_START, HEAP_START + HEAP_SIZE);
}
