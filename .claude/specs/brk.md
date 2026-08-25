# The brk syscall

Standard implementations of the brk syscall, follows standard UNIX API surface with:
```
int brk(void* end_data_segment);
void* sbrk(intptr_t increment);
```
These should be appended to the syscall list as syscalls 18 and 19.

### Where the heap is stored

The heap start should be determined during the ELF load process (and subsequently stored in the resulting `process` struct). The heap should follow immediately after the last ELF LOAD segment `max(vaddr + memsz)` + an offset of 0x1000 (one page). The heap should always only be touching while the process PML4 page table is loaded, and should only exist there. The heap is eager-stored since we do not have demand paging. The heap should also map to page granularity, then record the exact byte break. 

### Return values

On failure, the syscall returns a zero (0), which is never a valid program break. Otherwise, the new program break is returned.