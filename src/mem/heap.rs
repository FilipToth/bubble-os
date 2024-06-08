use core::{
    alloc::{AllocError, Allocator, Layout, GlobalAlloc},
    ptr::NonNull, ops::DerefMut,
};

use spinning_top::Spinlock;

use crate::{print, HEAP_ALLOCATOR};

pub const HEAP_START: usize = 0o_000_002_000_000_0000;
pub const HEAP_SIZE: usize = 1024 * 1024; // 1 MiB

struct Block {
    prev: Option<WrapperLocked<Block>>,
    next: Option<WrapperLocked<Block>>,
    used: bool,
    size: usize,
    address: usize
}

impl Block {
    fn new(address: usize, size: usize, prev: Option<WrapperLocked<Block>>) -> Block {
        Block { prev: prev, next: None, used: false, size: size, address: address }
    }
}

struct LinkedListHeap {
    heap_start: usize,
    heap_end: usize,
    size: usize,
    head: Option<WrapperLocked<Block>>,
}

impl LinkedListHeap {
    const fn empty() -> Self {
        Self { heap_start: 0, heap_end: 0, size: 0, head: None }
    }

/*     fn new(heap_start: usize, heap_end: usize) -> Self {
        let heap_size = heap_end - heap_start;
        let mut start_block = Block::new(heap_start, heap_size, None);
        let start_block_ptr = get_block_ptr(&mut start_block);

        Self {
            heap_start: heap_start,
            heap_end: heap_end,
            size: heap_size,
            head: start_block_ptr.clone(),
            start: start_block_ptr,
        }
    } */
}

pub struct LockedHeapAllocator(Spinlock<LinkedListHeap>);

impl LockedHeapAllocator {
    pub const fn empty() -> Self {
        LockedHeapAllocator(Spinlock::new(LinkedListHeap::empty()))
    }

    fn init(&mut self, heap_start: usize, heap_end: usize) {
        let heap_size = heap_end - heap_start;
        let mut start_block = Block::new(heap_start, heap_size, None);
        let start_block_ptr = get_block_ptr(&mut start_block);

        print!("[ OK ] heap_start_block: {:?}\n", start_block.address);

        // initialize heap spinlock
        {
            let mut heap = self.0.lock();
            heap.heap_start = heap_start;
            heap.heap_end = heap_end;
            heap.size = heap_size;
            heap.head = start_block_ptr;
        }

        self.verify_heap();
    }

    fn verify_heap(&self) {
        let heap = self.0.lock();

        let head = heap.head.clone();
        let head_is_some = head.is_some();

        let block = unsafe { head.unwrap().0.as_ref() };
        let next_is_some = block.next.is_some();

        print!("[ OK ] Running kernel heap verification\n");
        print!("[ OK ] head_is_some: {:?}\n", head_is_some);
        print!("[ OK ] head address: 0x{:x}\n", block.address);
        print!("[ OK ] next_is_some: {:?}\n", next_is_some);
    }

    fn allocate_internal(&self, layout: Layout) -> Result<*mut u8, AllocError> {
        let heap = self.0.lock();
        let head = heap.head.clone();
        let block = unsafe { head.unwrap().0.as_ref() };

        print!("[ OK ] block: 0x{:x}\n", block.address);

        return Ok(block.address as *mut u8);

/*         let heap = self.0.lock();
        print!("[ OK ] Alloc heap start: 0x{:x}\n", heap.heap_start);

        if heap.head.is_none() {
            return Err(AllocError);
        }

        let mut head = heap.head.clone();
        let temp = unsafe { head.clone().unwrap().0.as_ref() };
        print!("[ OK ] temp_a: 0x{:x}\n", temp.address);


        loop {
            if head.is_none() {
                break;
            }

            let mut block = unsafe { head.clone().unwrap().0.as_mut() };
            print!("block_addr: 0x{:x}\n", block.address);

            let alloc_start = align_up(block.address, layout.align());
            let alloc_end = alloc_start.saturating_add(layout.size());
            let aligned_size = alloc_end - alloc_start;

            if !block.used && block.size >= aligned_size {
                // found block, split up
                let block_size = block.size;
                let remainder_size = block_size - aligned_size;

                let remainder_addr = block.address + aligned_size;
                let mut remainder_next = Block::new(remainder_addr, remainder_size, head);

                let old_next = block.next.clone();
                remainder_next.next = old_next;

                block.next = get_block_ptr(&mut remainder_next);

                block.used = true;
                block.size = aligned_size;

                print!("alloc block: addr->0x{:x}, size->0x{:x}, next->{:?}\n", block.address, block.size, block.next.is_some());

                return Ok(block.address as *mut u8);
            }

            print!("[ ERR ] nextblock is_some: {:?}\n", block.next.is_some());
            head = block.next.clone();
        }

        return Err(AllocError);
        */
    }
}

unsafe impl<'a> Allocator for LockedHeapAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let block = self.allocate_internal(layout);
        if let Err(_) = block {
            return Err(AllocError)
        }

        let block = block.unwrap();
        let start_ptr = NonNull::<u8>::new(block).unwrap();
        let slice = NonNull::slice_from_raw_parts(start_ptr, layout.size());
        Ok(slice)
    }

    unsafe fn deallocate(&self, _ptr: NonNull<u8>, _layout: Layout) {
        todo!()
    }
}

unsafe impl GlobalAlloc for LockedHeapAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let block = self.allocate_internal(layout);
        match block {
            Ok(ptr) => ptr,
            Err(_) => core::ptr::null_mut()
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        todo!()
    }
}

fn get_block_ptr(block: &mut Block) -> Option<WrapperLocked<Block>> {
    Some(WrapperLocked(NonNull::<Block>::new(block as *mut _).unwrap()))
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

struct WrapperLocked<A>(NonNull<A>);
unsafe impl<A> Send for WrapperLocked<A> {}
unsafe impl<A> Sync for WrapperLocked<A> {}

impl<T> Clone for WrapperLocked<T> {
    fn clone(&self) -> Self {
        WrapperLocked(self.0.clone())
    }
}

pub unsafe fn init_heap() {
    HEAP_ALLOCATOR.init(HEAP_START, HEAP_START + HEAP_SIZE);
}
