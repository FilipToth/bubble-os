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

.PHONY: all full_build init_build clean kernel userspace libc disk iso \
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

userspace:
	$(MAKE) -C userspace

# The C library. newlib itself is a toolchain dependency built into the
# image, not source, so this only builds the porting layer that sits on top
# of it: crt0 and the syscall stubs.
libc:
	@if [ -d userspace/libc ]; then \
		$(MAKE) -C userspace/libc; \
	else \
		echo "userspace/libc does not exist yet, nothing to build"; \
	fi

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
