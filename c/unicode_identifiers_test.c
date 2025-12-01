#include <ctype.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define MAX_CODEPOINT 0x10ffffu
#define MAX_SCALAR (MAX_CODEPOINT + 1)

#include "unicode_identifiers.c"

static uint8_t derivedTable[MAX_SCALAR];

static char *trim(char *s)
{
	while (isspace((unsigned char)*s)) {
		s++;
	}

	if (*s == '\0') {
		return s;
	}

	char *end = s + strlen(s) - 1;
	while (end > s && isspace((unsigned char)*end)) {
		*end-- = '\0';
	}

	return s;
}

static FILE *open_derived_data(int argc, char **argv)
{
	if (argc > 1 && argv[1] && argv[1][0] != '\0') {
		FILE *file = fopen(argv[1], "r");
		if (file != NULL) {
			return file;
		}
	}

	const char *env = getenv("DERIVED_CORE_PROPERTIES");
	if (env && env[0] != '\0') {
		FILE *file = fopen(env, "r");
		if (file != NULL) {
			return file;
		}
	}

	const char *candidates[] = {"../DerivedCoreProperties.txt",
				    "../../DerivedCoreProperties.txt",
				    "DerivedCoreProperties.txt"};
	for (size_t i = 0; i < sizeof(candidates) / sizeof(candidates[0]);
	     ++i) {
		FILE *file = fopen(candidates[i], "r");
		if (file != NULL) {
			return file;
		}
	}

	return NULL;
}

static void load_derived_table(FILE *file)
{
	const size_t max_len = 1u << 15;
	char line[max_len];

	while (fgets(line, sizeof(line), file) != NULL) {
		char *hash = strchr(line, '#');
		if (hash) {
			*hash = '\0';
		}

		char *body = trim(line);
		if (*body == '\0') {
			continue;
		}

		char *semi = strchr(body, ';');
		if (semi == NULL) {
			continue;
		}
		*semi = '\0';
		char *range = trim(body);
		char *prop = trim(semi + 1);

		uint8_t bits = 0;
		if (strstr(prop, "ID_Start")) {
			bits |= IDENTIFIER_START;
		}
		if (strstr(prop, "ID_Continue")) {
			bits |= IDENTIFIER_CONTINUE;
		}
		if (bits == 0) {
			continue;
		}

		uint32_t start = 0;
		uint32_t end = 0;
		char *dots = strstr(range, "..");
		if (dots) {
			*dots = '\0';
			start = (uint32_t)strtoul(range, NULL, 16);
			end = (uint32_t)strtoul(dots + 2, NULL, 16);
		} else {
			start = (uint32_t)strtoul(range, NULL, 16);
			end = start;
		}

		if (start >= MAX_SCALAR) {
			continue;
		}
		if (end >= MAX_SCALAR) {
			end = MAX_SCALAR - 1;
		}

		for (uint32_t cp = start; cp <= end; ++cp) {
			derivedTable[cp] |= bits;
		}
	}
}

int main(int argc, char **argv)
{
	memset(derivedTable, 0, sizeof(derivedTable));
	FILE *derived = open_derived_data(argc, argv);
	if (derived == NULL) {
		fprintf(stderr,
			"failed to locate DerivedCoreProperties.txt; pass it as an argument or set DERIVED_CORE_PROPERTIES\n");
		return EXIT_FAILURE;
	}

	load_derived_table(derived);
	fclose(derived);

	for (uint32_t cp = 0; cp <= MAX_CODEPOINT; ++cp) {
		uint8_t expected = derivedTable[cp];
		if (cp >= 0x100000 && expected != 0) {
			fprintf(stderr,
				"derived data marks unsupported codepoint U+%04X\n",
				cp);
			return EXIT_FAILURE;
		}

		uint8_t actual = unicode_identifier_class(cp);
		if (actual != expected) {
			fprintf(stderr,
				"class mismatch at U+%04X: expected %u, got %u\n",
				cp, expected, actual);
			return EXIT_FAILURE;
		}
	}

	puts("unicode_identifier_class matches derived data");
	return EXIT_SUCCESS;
}
