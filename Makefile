arch ?= x86_64
kernel := build/kernel-$(arch).bin
iso := build/os-$(arch).iso
target ?= $(arch)-bubble-os
rust_os := target/$(target)/debug/libbubble_os.a

linker_script := src/arch/$(arch)/linker.ld
grub_cfg := src/arch/$(arch)/grub.cfg
assembly_source_files := $(wildcard src/arch/$(arch)/*.s)
assembly_object_files := $(patsubst src/arch/$(arch)/%.s, \
	build/arch/$(arch)/%.o, $(assembly_source_files))

.PHONY: all clean run iso kernel

all: $(kernel)

clean:
	cargo clean
	rm -r build

run: kernel_start iso run_without_building

debug_run: kernel_start run_wait_for_debugger

run_without_building:
	qemu-system-x86_64 -nographic -m 128M -cdrom $(iso) -s

run_wait_for_debugger:
	qemu-system-x86_64 -nographic -m 128M -cdrom $(iso) -s -S

gdb:
	gdb "$(kernel)" -ex "target remote :1234"

iso: $(iso)

$(iso): $(kernel) $(grub_cfg)
	mkdir -p build/isofiles/boot/grub
	cp $(kernel) build/isofiles/boot/kernel.bin
	cp $(grub_cfg) build/isofiles/boot/grub
	grub2-mkrescue -o $(iso) build/isofiles 2> /dev/null
# rm -r build/isofiles

$(kernel): kernel $(rust_os) $(assembly_object_files) $(linker_script)
	ld -n --gc-sections -T $(linker_script) -o $(kernel) build/arch/$(arch)/kernel_start.o $(assembly_object_files) $(rust_os)
kernel:
	RUST_TARGET_PATH=$$(pwd) xargo build --target $(target)

test: kernel_start_test $(iso) run_without_building

kernel_start:
	mkdir -p build/arch/$(arch)/
	echo "building: kernel_start"
	nasm -felf64 src/arch/$(arch)/kernel_start.asm -o build/arch/$(arch)/kernel_start.o

kernel_start_test:
	mkdir -p build/arch/$(arch)/
	echo "building: kernel_start_test"
	nasm -felf64 src/arch/$(arch)/kernel_start_test.asm -o build/arch/$(arch)/kernel_start.o

# compile assembly files
build/arch/$(arch)/%.o: src/arch/$(arch)/%.s
	mkdir -p $(shell dirname $@)
	echo $<
	nasm -felf64 $< -o $@