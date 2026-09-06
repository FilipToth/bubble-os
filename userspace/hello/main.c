/* First C program on bubble-os: proves the newlib port actually works.
 *
 * Each block below exercises a different part of the port rather than just
 * printing, so a failure narrows down to one syscall:
 *
 *   - puts/printf   -> _write, and newlib's stdio buffering on top of it
 *   - argc/argv     -> the entry frame crt0.S unpacks
 *   - getenv        -> environ, set by crt0.S from that same frame
 *   - malloc        -> _sbrk, and the kernel's brk implementation
 *   - fopen/fread   -> _open, _read, _lseek, _close, _fstat
 *   - printf %g     -> newlib's float formatting, and libm
 */

#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int main(int argc, char **argv)
{
	puts("hello from newlib");

	printf("argc: %d\n", argc);
	for (int i = 0; i < argc; i++) {
		printf("argv[%d]: %s\n", i, argv[i]);
	}

	const char *path = getenv("PATH");
	printf("PATH: %s\n", path != NULL ? path : "(unset)");

	/* The first malloc is what forces newlib to call _sbrk, so this is
	 * really a test of the brk syscall wearing a heap as a disguise. */
	char *buffer = malloc(64);
	if (buffer == NULL) {
		puts("malloc failed");
		return 1;
	}

	snprintf(buffer, 64, "malloc and snprintf agree: %d", 6 * 7);
	puts(buffer);
	free(buffer);

	/* %.14g is how Lua prints every number and strtod is how it reads them,
	 * so this is the acceptance test for the whole floating point path: the
	 * FPU state the scheduler switches, newlib built with float printf, and
	 * libm on the link line. Getting it wrong is silent, the numbers just
	 * come out as garbage or as a literal "g", with no build error.
	 *
	 * Expect: 3.14159265358979, 2, 0.5 */
	double pi = strtod("3.14159265358979", NULL);
	printf("%.14g %.14g %.14g\n", pi, floor(2.7), fmod(6.5, 3.0));

	/* Only attempted when given a path, so the program is still useful
	 * with no arguments. */
	if (argc > 1) {
		FILE *file = fopen(argv[1], "r");
		if (file == NULL) {
			printf("could not open %s\n", argv[1]);
			return 1;
		}

		char line[128];
		if (fgets(line, sizeof(line), file) != NULL) {
			printf("first line of %s: %s", argv[1], line);

			/* fgets keeps the newline, unless the line was longer
			 * than the buffer or the file does not end in one */
			if (strchr(line, '\n') == NULL) {
				putchar('\n');
			}
		} else {
			printf("%s is empty\n", argv[1]);
		}

		fclose(file);
	}

	/* exit() flushes too, this just makes the output ordering explicit. */
	fflush(stdout);
	return 0;
}
