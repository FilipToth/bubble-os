.PHONY: all sample sample2 tempshell shell ls cat prepare

all: prepare sample sample2 shell ls cat

prepare:
	mkdir -p bin
	mkdir -p obj

sample:
	nasm -f elf64 sample/main.s -o obj/sample.o
	ld -T sample/linker.ld -m elf_x86_64 obj/sample.o -o bin/sample.elf

sample2:
	nasm -f elf64 sample2/main.s -o obj/sample2.o
	ld -T sample2/linker.ld -m elf_x86_64 obj/sample2.o -o bin/sample2.elf

tempshell:
	nasm -f elf64 tempshell/shell.s -o obj/tempshell.o
	ld -T tempshell/linker.ld -m elf_x86_64 obj/tempshell.o -o bin/tshell.elf

shell:
	cargo rustc --manifest-path shell/Cargo.toml -Z build-std=core,compiler_builtins -Z build-std-features=compiler-builtins-mem --target shell/x86_64-bubble-userspace.json --release -- -C linker=ld -C link-arg=-T -C link-arg=linker.ld -C link-arg=-m -C link-arg=elf_x86_64
	cp shell/target/x86_64-bubble-userspace/release/shell bin/shell.elf

ls:
	nasm -f elf64 ls/ls.s -o obj/ls.o
	ld -T ls/linker.ld -m elf_x86_64 obj/ls.o -o bin/ls.elf

cat:
	nasm -f elf64 cat/cat.s -o obj/cat.o
	ld -T cat/linker.ld -m elf_x86_64 obj/cat.o -o bin/cat.elf
