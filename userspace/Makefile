.PHONY: all sample prepare

all: prepare sample

prepare:
	mkdir -p bin
	mkdir -p obj

sample:
	nasm -f elf32 sample/main.s -o obj/sample.o
	ld -m elf_i386 obj/sample.o -o bin/sample.elf