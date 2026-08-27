# Host entry point. Everything that actually compiles runs in the container,
# which is the only place with GNU ld, nasm, mtools and grub. The build rules
# themselves live in build.mk; this file just forwards to them.
#
# The exception is gdb, which runs here and attaches to the QEMU stub the
# container publishes on port 1234.

arch ?= x86_64
kernel := build/kernel-$(arch).bin

service := builder
compose := docker compose
in_container := $(compose) exec -T $(service) make -f build.mk

# Targets that only produce files. No TTY, so output stays clean and this
# still works somewhere without one
build_targets := kernel userspace libc disk iso full_build clean

# Targets that hand the terminal to QEMU, which needs a real TTY to drive
# its serial console
run_targets := run build_and_run int_run debug_run

.PHONY: up down shell gdb $(build_targets) $(run_targets)

$(build_targets): up
	$(in_container) $@

$(run_targets): up
	$(compose) exec $(service) make -f build.mk $@

# idempotent, and cheap once the container is already running
up:
	$(compose) up -d $(service)

down:
	$(compose) down

# a shell in the build container, for poking at things by hand
shell: up
	$(compose) exec $(service) bash

# Attaches to QEMU running inside the container. Start it with `make
# debug_run` in another terminal first, which stops before the first
# instruction and waits
gdb:
	gdb "$(kernel)" -ex "target remote :1234"
