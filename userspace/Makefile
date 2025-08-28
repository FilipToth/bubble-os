.PHONY: all sample sample2 shell ls prepare

all: prepare sample sample2 shell ls

prepare:
	mkdir -p bin
	mkdir -p obj

sample:
	nasm -f elf64 sample/main.s -o obj/sample.o
	ld -T sample/linker.ld -m elf_x86_64 obj/sample.o -o bin/sample.elf

sample2:
	nasm -f elf64 sample2/main.s -o obj/sample2.o
	ld -T sample2/linker.ld -m elf_x86_64 obj/sample2.o -o bin/sample2.elf

shell:
	nasm -f elf64 shell/shell.s -o obj/shell.o
	ld -T shell/linker.ld -m elf_x86_64 obj/shell.o -o bin/shell.elf

ls:
	nasm -f elf64 ls/ls.s -o obj/ls.o
	ld -T ls/linker.ld -m elf_x86_64 obj/ls.o -o bin/ls.elf
