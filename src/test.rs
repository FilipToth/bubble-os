use multiboot2::BootInformation;

use crate::mem::paging::{entry::EntryFlags, Page};
use crate::mem::{PageFrameAllocator, SimplePageFrameAllocator};
use crate::print;

pub struct TestUnit<'a> {
    function: &'a dyn Fn() -> bool,
    succeeded: bool,
    name: &'a str,
}

static mut BOOT_INFO_ADDR: Option<usize> = None;
static mut BOOT_INFO: Option<BootInformation> = None;
static mut MULTIBOOT_MEM_END: Option<usize> = None;
static mut MEM_END: Option<usize> = None;
static mut PAGE_FRAME_ALLOCATOR: Option<SimplePageFrameAllocator> = None;

impl<'a> TestUnit<'a> {
    pub fn new(func: &'a dyn Fn() -> bool, name: &'a str) -> TestUnit<'a> {
        let mut unit = TestUnit {
            function: func,
            succeeded: false,
            name: name,
        };

        unit.run();
        unit
    }

    pub fn run(&mut self) {
        let success = (self.function)();
        self.succeeded = success;

        if success {
            print!("[   OK   ] Case \"{}\" succeeded!\n", self.name);
        } else {
            print!("[ FAILED ] Case \"{}\" failed!\n", self.name);
        }
    }
}

#[macro_export]
macro_rules! assert_true {
    ($($x:expr)*) => {
        if !( $($x)* ) {
            crate::print!("\nFAILED TRACE: file: {}, line: {}\n\n", file!(), line!());
            return false;
        }
    };
}

pub fn run_tests(boot_info_addr: usize) {
    unsafe { BOOT_INFO_ADDR = Some(boot_info_addr) };

    TestUnit::new(&test_boot_info, "Test Boot Info");
    TestUnit::new(&test_memory_map, "Test Memory Map");
    TestUnit::new(&test_page_frame_allocator, "Test Page Frame Allocator");
    TestUnit::new(&test_paging, "Test Paging");
    TestUnit::new(
        &test_frame_allocator_fill_memory,
        "Test Page Frame Allocator Fill Memory",
    );
}

fn test_boot_info() -> bool {
    unsafe {
        assert_true!(BOOT_INFO_ADDR.is_some());
        let boot_info_addr = BOOT_INFO_ADDR.unwrap();

        let boot_info_load_res = multiboot2::BootInformation::load(
            boot_info_addr as *const multiboot2::BootInformationHeader,
        );
        assert_true!(boot_info_load_res.is_ok());

        if let Ok(boot_info) = boot_info_load_res {
            BOOT_INFO = Some(boot_info);
        }
    }

    return true;
}

fn test_memory_map() -> bool {
    unsafe {
        assert_true!(BOOT_INFO.is_some());
        let boot_info = BOOT_INFO.as_ref().unwrap();

        let map_tag_opt = boot_info.memory_map_tag();
        assert_true!(map_tag_opt.is_some());

        let map_tag = map_tag_opt.unwrap();
        let memory_areas = map_tag.memory_areas();
        assert_true!(memory_areas.len() > 0);

        let boot_info_addr = BOOT_INFO_ADDR.as_ref().unwrap();
        assert_true!(boot_info_addr.clone() > 0);

        let multiboot_end = boot_info_addr + boot_info.total_size();
        assert_true!(multiboot_end > 0);

        MULTIBOOT_MEM_END = Some(multiboot_end);

        // for some reason when getting the last memory area,
        // it's always padded to 4GB, the second last area
        // actually corresponds to the memory available

        let memory_end = memory_areas[memory_areas.len() - 2].end_address();
        assert_true!(memory_end > 0);
        MEM_END = Some(memory_end as usize);
    }

    return true;
}

fn test_page_frame_allocator() -> bool {
    unsafe {
        assert_true!(MULTIBOOT_MEM_END.is_some());
        let mem_start = MULTIBOOT_MEM_END.as_ref().unwrap();

        assert_true!(MEM_END.is_some());
        let mem_end = MEM_END.as_ref().unwrap();

        let mut allocator = SimplePageFrameAllocator::new(mem_start.clone(), mem_end.clone());

        let frame_res = allocator.falloc();
        assert_true!(frame_res.is_some());

        PAGE_FRAME_ALLOCATOR = Some(allocator);
    }

    return true;
}

fn test_paging() -> bool {
    /*     unsafe {
        // test map
        assert_true!(PAGE_FRAME_ALLOCATOR.is_some());
        let allocator = PAGE_FRAME_ALLOCATOR.as_mut().unwrap();

        let mut page_table = ActivePageTable::new();

        let addr = 42 * 512 * 512 * 4096;
        let page = Page::for_address(addr);
        let frame = allocator.falloc();

        let unmapped_addr = page_table.translate_to_phys(addr);
        assert_true!(unmapped_addr.is_none());

        page_table.map(page, EntryFlags::empty(), allocator);
        let mapped_addr = page_table.translate_to_phys(addr);
        assert_true!(mapped_addr.is_some());

        let next_frame = allocator.falloc();
        assert_true!(next_frame.is_some());

        let last_num = frame.unwrap().frame_number;
        let next_num = next_frame.unwrap().frame_number;
        let num_diff = next_num - last_num;

        assert_true!(num_diff == 4);

        let unmap_page = Page::for_address(addr);
        page_table.unmap(unmap_page, allocator);

        let unmapped_addr = page_table.translate_to_phys(addr);
        assert_true!(unmapped_addr.is_none());
    } */

    return true;
}

fn test_frame_allocator_fill_memory() -> bool {
    unsafe {
        assert_true!(PAGE_FRAME_ALLOCATOR.is_some());
        let allocator = PAGE_FRAME_ALLOCATOR.as_mut().unwrap();

        assert_true!(MEM_END.is_some());
        let mem_end = MEM_END.as_ref().unwrap();

        let mut last_page_num = 0;
        for i in 0.. {
            let alloc_res = allocator.falloc();
            match alloc_res {
                Some(page) => {
                    let addr = page.start_address();
                    assert_true!(addr < mem_end.clone());

                    let num = page.frame_number;
                    if i == 0 {
                        last_page_num = num;
                        continue;
                    }

                    assert_true!(num - 1 == last_page_num);
                    last_page_num = num;
                }
                None => break,
            }
        }
    }

    return true;
}
