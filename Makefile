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
resources := $(wildcard resources/*)
user_binaries := $(wildcard userspace/bin/*)
base_qemu := qemu-system-x86_64 -nographic -serial mon:stdio -m 256M -cdrom $(iso) -boot d -s -no-reboot -machine q35 -drive file=$(disk_path),if=none,id=disk0,format=raw -device ahci,id=ahci -device ide-hd,drive=disk0,bus=ahci.0

.PHONY: all clean run iso kernel disk userspace

all: $(kernel)

clean:
	cargo clean
	rm -r build

run: userspace disk kernel_start iso run_without_building
int_run: userspace disk kernel_start iso run_without_building_debug_interrupts
debug_run: userspace disk kernel_start iso run_wait_for_debugger

run_without_building:
	$(base_qemu)

run_without_building_debug_interrupts:
	$(base_qemu) -d int

run_wait_for_debugger:
	$(base_qemu) -S

gdb:
	gdb "$(kernel)" -ex "target remote :1234"

iso: $(iso)

disk:
	qemu-img create -f raw $(disk_path) 128M
	mkfs.vfat -F 32 -v $(disk_path)

	@for file in $(resources); do \
		echo $$(basename $$file); \
		mcopy -i $(disk_path) "$$file" ::$$(basename $$file); \
	done

	@for file in $(user_binaries); do \
		echo $$(basename $$file); \
		mcopy -i $(disk_path) "$$file" ::$$(basename $$file); \
	done

userspace:
	make -C userspace

$(iso): $(kernel) $(grub_cfg)
	mkdir -p build/isofiles/boot/grub
	cp $(kernel) build/isofiles/boot/kernel.bin
	cp $(grub_cfg) build/isofiles/boot/grub
	grub2-mkrescue -o $(iso) build/isofiles 2> /dev/null
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