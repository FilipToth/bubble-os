# Build rules that run inside the container.
#
# Nothing here works on a macOS host: it needs GNU ld rather than Apple's,
# nasm, mtools, mkfs.vfat and grub-mkrescue. The host Makefile forwards to
# this file through `docker compose exec` and never runs these rules directly.

arch ?= x86_64
kernel := build/kernel-$(arch).bin
iso := build/os-$(arch).iso
target ?= $(arch)-bubble-os
rust_os := target/$(target)/debug/libbubble_os.a
disk_path := build/disk.img

linker_script := src/arch/$(arch)/boot/linker.ld
grub_cfg := src/arch/$(arch)/boot/grub.cfg
assembly_source_files := $(wildcard src/arch/$(arch)/boot/*.s)
assembly_object_files := $(patsubst src/arch/$(arch)/boot/%.s, \
						 build/arch/$(arch)/boot/%.o, $(assembly_source_files))

base_qemu := qemu-system-x86_64 \
			 -nographic -serial mon:stdio \
			 -m 256M \
			 -cdrom $(iso) \
			 -boot d \
			 -s \
			 -no-reboot \
			 -machine q35 \
			 -drive file=$(disk_path),if=none,id=disk0,format=raw \
			 -device ahci,id=ahci \
			 -device ide-hd,drive=disk0,bus=ahci.0 \
			 -netdev socket,id=n0,udp=127.0.0.1:1234,localaddr=127.0.0.1:1235 \
			 -device e1000,netdev=n0

grub_rescue := $(shell command -v grub2-mkrescue >/dev/null 2>&1 && echo grub2-mkrescue || echo grub-mkrescue)

.PHONY: all full_build init_build clean kernel userspace libc newlib libc_clean hello disk iso \
		kernel_start kernel_start_test test run run_w_debug_interrupts \
		debug_run build_and_run int_run

all: $(kernel)

full_build: init_build userspace disk kernel_start iso

init_build:
	mkdir -p build

clean:
	cargo clean
	$(MAKE) -C userspace clean
	rm -rf build

build_and_run: userspace disk kernel_start iso run
int_run: userspace disk kernel_start iso run_w_debug_interrupts

# QEMU stops before the first instruction and waits for a debugger. The gdb
# stub is published to the host, so `make gdb` runs out there
debug_run:
	@echo "Starting QEMU and waiting for debugger..."
	@$(base_qemu) -S

run:
	$(base_qemu)

run_w_debug_interrupts:
	$(base_qemu) -d int

iso: $(iso)

disk:
	qemu-img create -f raw $(disk_path) 128M
	mkfs.vfat -F 32 -v $(disk_path)

	mmd -i $(disk_path) ::res
	mmd -i $(disk_path) ::res/dir
	mmd -i $(disk_path) ::bin

	@# globbed by the shell rather than by $(wildcard). make caches directory
	@# listings for the whole invocation, so a make level glob here would
	@# still see userspace/bin as it was before `userspace` filled it
	@for file in resources/*; do \
		[ -e "$$file" ] || continue; \
		echo $$(basename $$file); \
		mcopy -i $(disk_path) "$$file" ::res/$$(basename $$file); \
	done

	@for file in userspace/bin/*; do \
		[ -e "$$file" ] || continue; \
		echo $$(basename $$file); \
		mcopy -i $(disk_path) "$$file" ::bin/$$(basename $$file); \
	done

# Depends on libc because `all` over there now includes the C programs, which
# need build/libc/libc.a and the x86_64-elf toolchain to exist first.
userspace: libc
	$(MAKE) -C userspace

# ---------------------------------------------------------------------------
# C library
#
# Two halves. newlib is a toolchain dependency: its source is unpacked into
# the image at /opt/newlib and it builds into a prefix outside the repo, so
# nothing about it lands in the source tree. Our porting layer is real
# source and lives in userspace/lib.
#
# They are joined by copying newlib's libc.a and adding syscalls.o to it.
# --disable-newlib-supplied-syscalls leaves the `_*` symbols undefined in
# libc.a, and putting the definitions in the same archive lets ld resolve
# them in its normal repeated passes over a single archive. Keeping them in
# a separate library would work too, but only with --start-group, since
# syscalls.c calls back into libc for memset and strlen.
# ---------------------------------------------------------------------------

newlib_version ?= 4.6.0.20260123
newlib_src := /opt/newlib/newlib-$(newlib_version)
newlib_build := /opt/newlib/build

toolchain := /opt/bubble-toolchain
cross := x86_64-elf
cross_lib := $(toolchain)/$(cross)/lib
cross_include := $(toolchain)/$(cross)/include

libc_out := build/libc

# -mcmodel=large: user programs link at 0x0000700040000000. The default small
# model assumes every symbol sits in the low 2GB and emits absolute 32-bit
# relocations for them, which cannot hold an address that high, so the link
# dies with "relocation truncated to fit: R_X86_64_32". The large model uses
# 64-bit absolute references throughout and works at any address. The Rust
# crates avoid this without a code model flag only because rustc defaults to
# a PIC relocation model and addresses everything rip-relative.
libc_cflags := -O2 -g -std=gnu11 -ffreestanding -fno-stack-protector \
			   -mcmodel=large -Wall -Wextra -I$(cross_include)

# newlib itself has to be built the same way, or its own objects carry the
# relocations that cannot be resolved.
newlib_target_cflags := -O2 -g -mcmodel=large

# Configure is guarded on the build directory rather than declared as a
# prerequisite: newlib decides for itself what is out of date, and letting
# make second-guess it just reruns a five minute configure for nothing.
# Delete /opt/newlib/build to force a reconfigure after changing the flags.
newlib:
	@if [ ! -d "$(newlib_src)" ]; then \
		echo "newlib source missing at $(newlib_src)."; \
		echo "The image predates it, rebuild with: docker compose build builder"; \
		exit 1; \
	fi
	@if [ ! -f "$(newlib_build)/Makefile" ]; then \
		echo "configuring newlib $(newlib_version)"; \
		mkdir -p $(newlib_build); \
		cd $(newlib_build) && CFLAGS_FOR_TARGET="$(newlib_target_cflags)" \
			$(newlib_src)/configure \
			--target=$(cross) \
			--prefix=$(toolchain) \
			--disable-multilib \
			--disable-nls \
			--disable-newlib-supplied-syscalls; \
	fi
	$(MAKE) -C $(newlib_build) CFLAGS_FOR_TARGET="$(newlib_target_cflags)" -j$(shell nproc)
	$(MAKE) -C $(newlib_build) CFLAGS_FOR_TARGET="$(newlib_target_cflags)" install

libc: newlib
	mkdir -p $(libc_out)
	$(cross)-gcc $(libc_cflags) -c userspace/lib/syscalls.c -o $(libc_out)/syscalls.o
	$(cross)-gcc $(libc_cflags) -c userspace/lib/crt0.S -o $(libc_out)/crt0.o
	cp $(cross_lib)/libc.a $(libc_out)/libc.a
	$(cross)-ar rcs $(libc_out)/libc.a $(libc_out)/syscalls.o
	@echo "libc: $(libc_out)/libc.a"

libc_clean:
	rm -rf $(libc_out) $(newlib_build)

# The first C program, and the acceptance test for the whole libc chain.
# `userspace` builds this too; the standalone target is for iterating on it
# without rebuilding the five Rust crates.
hello: libc
	$(MAKE) -C userspace hello cross=$(cross) toolchain=$(toolchain) \
		libc_dir=$(CURDIR)/$(libc_out)

$(iso): $(kernel) $(grub_cfg)
	mkdir -p build/isofiles/boot/grub
	cp $(kernel) build/isofiles/boot/kernel.bin
	cp $(grub_cfg) build/isofiles/boot/grub
	$(grub_rescue) -o $(iso) build/isofiles 2> /dev/null
# rm -r build/isofiles

$(kernel): kernel $(rust_os) $(assembly_object_files) $(linker_script)
	ld -n --gc-sections -T $(linker_script) -o $(kernel) build/arch/$(arch)/boot/kernel_start.o $(assembly_object_files) $(rust_os)

kernel:
	RUST_TARGET_PATH=$(shell pwd) xargo build --target $(target)

test: kernel_start_test $(iso) run_without_building

kernel_start:
	mkdir -p build/arch/$(arch)/boot/
	echo "building: kernel_start"
	nasm -felf64 src/arch/$(arch)/boot/kernel_start.asm -o build/arch/$(arch)/boot/kernel_start.o

kernel_start_test:
	mkdir -p build/arch/$(arch)/boot/
	echo "building: kernel_start_test"
	nasm -felf64 src/arch/$(arch)/boot/kernel_start_test.asm -o build/arch/$(arch)/boot/kernel_start.o

# compile assembly files
build/arch/$(arch)/boot/%.o: src/arch/$(arch)/boot/%.s
	mkdir -p $(shell dirname $@)
	echo $<
	nasm -felf64 $< -o $@
