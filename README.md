# Bubble-OS

Bubble-OS is a operating system kernel written in Rust for the x86-64 architecture. It currently functions as a headless operating system, but will be extended to provide tier-1 hypervisor support through Intel's VT-x virtualization technology. The main focus of this project is to explore how to cleanly tie different kernel subsystems together in a clean monolithic kernel design pattern.

### Features

- Grub Multiboot entry-point written in assembly
- Protected Mode => Long Mode transition
- Serial port interface
- 64-bit recursive paging (in progress to switch to a more extendable architecture)
- Kernel Heap
- GDT and TSS
- Hardware and Software Interrupts
- CPU exception handling
- PCI device enumeration
- ACPI table parsing
- AHCI and SATA disk support
- FAT32 filesystem driver
- ELF loader
- Round-robin scheduler
- Userspace and ring3 initialization
- Ring3 process context switches
- Ring3-compatible syscall interface
- Ring3-specific page tables and process memory isolation (in progress; need to first rework page table architecture)

### A Note on Code Quality

We also require strict code quality standards to ensure that the codebase remains readable, and thus can act as a reference point for future kernel-level projects. In practice, this means clearly separating kernel functions into different modules with clear and descriptive names, unlike some OS projects, which simply group all of their modules in one root directory.

### Development Notes

- **Docker:** There is a Docker Compose available to streamline the compilation of the kernel and the preparation of the OS ISO in a controlled environment.

- **GDB Stub Setup:** If you need to set up a GDB stub to communicate with QEMU, you'll need to compile GDB 12.1 from source while applying this [patch](https://github.com/mduft/tachyon3/blob/master/tools/patches/gdb-12.1-archswitch.patch).

- **Using Xargo:** Rust's favorite cross-compilation sysroot manager currently experiences a bug which causes it to look for the `Cargo.lock` file in the wrong directory. For more info on the issue, please refer to [Xargo Issue #347](https://github.com/japaric/xargo/issues/347), you can also find a simple patch applying in this repo: [FilipToth/xargo](https://github.com/FilipToth/xargo).
