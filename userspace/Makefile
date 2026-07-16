.PHONY: all sample sample2 tempshell shell edit ls cat prepare

all: prepare sample sample2 shell edit ls cat

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

edit:
	cargo rustc --manifest-path edit/Cargo.toml -Z build-std=core,compiler_builtins -Z build-std-features=compiler-builtins-mem --target edit/x86_64-bubble-userspace.json --release -- -C linker=ld -C link-arg=-T -C link-arg=linker.ld -C link-arg=-m -C link-arg=elf_x86_64
	cp edit/target/x86_64-bubble-userspace/release/edit bin/edit.elf

ls:
	cargo rustc --manifest-path ls/Cargo.toml -Z build-std=core,compiler_builtins -Z build-std-features=compiler-builtins-mem --target ls/x86_64-bubble-userspace.json --release -- -C linker=ld -C link-arg=-T -C link-arg=linker.ld -C link-arg=-m -C link-arg=elf_x86_64
	cp ls/target/x86_64-bubble-userspace/release/ls bin/ls.elf

cat:
	cargo rustc --manifest-path cat/Cargo.toml -Z build-std=core,compiler_builtins -Z build-std-features=compiler-builtins-mem --target cat/x86_64-bubble-userspace.json --release -- -C linker=ld -C link-arg=-T -C link-arg=linker.ld -C link-arg=-m -C link-arg=elf_x86_64
	cp cat/target/x86_64-bubble-userspace/release/cat bin/cat.elf
