.PHONY: all sample sample2 faulttest fputest brktest tempshell shell edit ls cat statinfo hello prepare clean

all: prepare sample sample2 faulttest fputest brktest shell edit ls cat statinfo hello

prepare:
	mkdir -p bin
	mkdir -p obj

sample:
	nasm -f elf64 sample/main.s -o obj/sample.o
	ld -T sample/linker.ld -m elf_x86_64 -z max-page-size=0x1000 obj/sample.o -o bin/sample.elf

sample2:
	nasm -f elf64 sample2/main.s -o obj/sample2.o
	ld -T sample2/linker.ld -m elf_x86_64 -z max-page-size=0x1000 obj/sample2.o -o bin/sample2.elf

faulttest:
	nasm -f elf64 faulttest/main.s -o obj/faulttest.o
	ld -T faulttest/linker.ld -m elf_x86_64 -z max-page-size=0x1000 obj/faulttest.o -o bin/faulttest.elf

fputest:
	nasm -f elf64 fputest/main.s -o obj/fputest.o
	ld -T fputest/linker.ld -m elf_x86_64 -z max-page-size=0x1000 obj/fputest.o -o bin/fputest.elf

brktest:
	nasm -f elf64 brktest/main.s -o obj/brktest.o
	ld -T brktest/linker.ld -m elf_x86_64 -z max-page-size=0x1000 obj/brktest.o -o bin/brktest.elf

tempshell:
	nasm -f elf64 tempshell/shell.s -o obj/tempshell.o
	ld -T tempshell/linker.ld -m elf_x86_64 -z max-page-size=0x1000 obj/tempshell.o -o bin/tshell.elf

shell:
	cargo rustc --manifest-path shell/Cargo.toml -Z build-std=core,compiler_builtins -Z build-std-features=compiler-builtins-mem --target shell/x86_64-bubble-userspace.json --release -- -C linker=ld -C link-arg=-T -C link-arg=linker.ld -C link-arg=-m -C link-arg=elf_x86_64 -C link-arg=-z -C link-arg=max-page-size=0x1000
	cp shell/target/x86_64-bubble-userspace/release/shell bin/shell.elf

edit:
	cargo rustc --manifest-path edit/Cargo.toml -Z build-std=core,compiler_builtins -Z build-std-features=compiler-builtins-mem --target edit/x86_64-bubble-userspace.json --release -- -C linker=ld -C link-arg=-T -C link-arg=linker.ld -C link-arg=-m -C link-arg=elf_x86_64 -C link-arg=-z -C link-arg=max-page-size=0x1000
	cp edit/target/x86_64-bubble-userspace/release/edit bin/edit.elf

ls:
	cargo rustc --manifest-path ls/Cargo.toml -Z build-std=core,compiler_builtins -Z build-std-features=compiler-builtins-mem --target ls/x86_64-bubble-userspace.json --release -- -C linker=ld -C link-arg=-T -C link-arg=linker.ld -C link-arg=-m -C link-arg=elf_x86_64 -C link-arg=-z -C link-arg=max-page-size=0x1000
	cp ls/target/x86_64-bubble-userspace/release/ls bin/ls.elf

cat:
	cargo rustc --manifest-path cat/Cargo.toml -Z build-std=core,compiler_builtins -Z build-std-features=compiler-builtins-mem --target cat/x86_64-bubble-userspace.json --release -- -C linker=ld -C link-arg=-T -C link-arg=linker.ld -C link-arg=-m -C link-arg=elf_x86_64 -C link-arg=-z -C link-arg=max-page-size=0x1000
	cp cat/target/x86_64-bubble-userspace/release/cat bin/cat.elf

statinfo:
	cargo rustc --manifest-path statinfo/Cargo.toml -Z build-std=core,compiler_builtins -Z build-std-features=compiler-builtins-mem --target statinfo/x86_64-bubble-userspace.json --release -- -C linker=ld -C link-arg=-T -C link-arg=linker.ld -C link-arg=-m -C link-arg=elf_x86_64 -C link-arg=-z -C link-arg=max-page-size=0x1000
	cp statinfo/target/x86_64-bubble-userspace/release/statinfo bin/statinfo.elf

# ---------------------------------------------------------------------------
# C programs
#
# These need build/libc/libc.a and the cross toolchain, neither of which this
# makefile can produce. build.mk's `userspace` target depends on `libc` and
# so guarantees both before recursing in here.
#
# The defaults below are what those paths are when make runs from the repo
# root, so `make -C userspace` also works directly once libc exists.
# ---------------------------------------------------------------------------

cross ?= x86_64-elf
toolchain ?= /opt/bubble-toolchain
libc_dir ?= ../build/libc

# -mcmodel=large: user programs link at 0x0000700040000000. The default small
# model assumes every symbol sits in the low 2GB and emits absolute 32-bit
# relocations for them, which cannot hold an address that high, so the link
# dies with "relocation truncated to fit: R_X86_64_32". The large model uses
# 64-bit absolute references throughout and works at any address. The Rust
# crates avoid this without a code model flag only because rustc defaults to
# a PIC relocation model and addresses everything rip-relative.
cflags := -O2 -g -std=gnu11 -ffreestanding -fno-stack-protector \
		  -mcmodel=large -Wall -Wextra -I$(toolchain)/$(cross)/include

ldflags := -nostdlib -static -Wl,-z,max-page-size=0x1000

# --start-group because these call into each other: libc needs gcc's 128-bit
# division helpers, libgcc needs memcpy, and libm needs both.
#
# -lm is separate from the merged libc.a on purpose. newlib keeps floor, ceil,
# fmod, sqrt, pow, log and the trig functions in libm; only ldexp, frexp and
# modf are in libc. Anything doing real arithmetic needs it, and leaving it out
# shows up as undefined references rather than anything subtle.
libs := -Wl,--start-group $(libc_dir)/libc.a -lm -lgcc -Wl,--end-group

hello: prepare
	$(cross)-gcc $(cflags) -c hello/main.c -o obj/hello.o
	$(cross)-gcc $(ldflags) -T hello/linker.ld \
		$(libc_dir)/crt0.o obj/hello.o \
		$(libs) \
		-o bin/hello.elf

clean:
	rm -rf bin obj
	@for crate in shell edit ls cat statinfo; do \
		cargo clean --manifest-path $$crate/Cargo.toml; \
	done
