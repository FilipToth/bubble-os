.PHONY: all sample prepare

all: prepare sample

prepare:
	mkdir -p bin
	mkdir -p obj

sample:
	nasm -f elf64 sample/main.s -o obj/sample.o
	ld -m elf_x86_64 obj/sample.o -o bin/sample.elf