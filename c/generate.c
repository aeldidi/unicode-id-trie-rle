#include <ctype.h>
#include <inttypes.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

enum {
	MAX_CODEPOINT = 0x0fffff,
	START_CODEPOINT = 0x80,
	SHIFT = 10,
	TOP_BITS = 6,
	BYTE_VALUES_PER_LINE = 12,
	INDEX_VALUES_PER_LINE = 8,
	MAX_UINT16 = 0xffffu,
	MAX_RUNS = 8192,
	MAX_LEAF_RUNS = 16384,
	MAX_LEAVES = 4096,
	MAX_LOCAL_RUNS = 256,
	MAX_LEVEL2_ENTRIES = 2048,
};

struct run {
	uint32_t start;
	uint8_t value;
};

struct leaf_run {
	uint16_t start;
	uint8_t value;
};

struct leaf_entry {
	uint16_t offset;
	uint16_t len;
};

static uint8_t all_table[MAX_CODEPOINT + 1];
static struct run runs[MAX_RUNS];
static struct leaf_run leaf_runs[MAX_LEAF_RUNS];
static struct leaf_entry leaves[MAX_LEAVES];
static size_t block_index[1024];
static uint16_t block_to_leaf[1024];
static uint16_t level2_tables[MAX_LEVEL2_ENTRIES];
static uint16_t level1_table[64];
static struct leaf_run local_runs[MAX_LOCAL_RUNS];
static uint8_t ascii_table[START_CODEPOINT];

static void bail(const char *msg)
{
	fprintf(stderr, "generate: %s\n", msg);
	exit(EXIT_FAILURE);
}

static char *trim(char *s)
{
	while (*s != '\0' && isspace((unsigned char)*s)) {
		++s;
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

static void parse_range(char *range, uint32_t *start, uint32_t *end)
{
	char *dots = strstr(range, "..");
	if (dots != NULL) {
		*dots = '\0';
		*start = (uint32_t)strtoul(range, NULL, 16);
		*end = (uint32_t)strtoul(dots + 2, NULL, 16);
	} else {
		*start = (uint32_t)strtoul(range, NULL, 16);
		*end = *start;
	}
}

static void load_table(FILE *file, uint8_t *table, size_t table_len)
{
	const size_t max_len = 1u << 15;
	char line[max_len];

	memset(table, 0, table_len);
	while (fgets(line, sizeof(line), file) != NULL) {
		char *hash = strchr(line, '#');
		if (hash != NULL) {
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
			bits |= 1;
		}
		if (strstr(prop, "ID_Continue")) {
			bits |= 2;
		}
		if (bits == 0) {
			continue;
		}

		uint32_t start = 0;
		uint32_t end = 0;
		parse_range(range, &start, &end);
		if (start > MAX_CODEPOINT) {
			continue;
		}
		if (end > MAX_CODEPOINT) {
			end = MAX_CODEPOINT;
		}

		for (uint32_t cp = start; cp <= end; ++cp) {
			table[cp] |= bits;
		}
	}
}

static uint32_t bit_width(uint32_t value)
{
	uint32_t bits = 0;
	while (value != 0) {
		++bits;
		value >>= 1;
	}
	return bits;
}

static bool leaves_equal(const struct leaf_run *leaf_runs,
			 struct leaf_entry entry, const struct leaf_run *local,
			 size_t local_len)
{
	if (entry.len != local_len) {
		return false;
	}

	return memcmp(leaf_runs + entry.offset, local,
		      local_len * sizeof(*local))
	       == 0;
}

static void emit_u16_array(const char *name, const uint16_t *data, size_t len,
			   size_t per_line)
{
	printf("static const uint16_t %s[%zu] = {\n", name, len);
	for (size_t i = 0; i < len; ++i) {
		if (i % per_line == 0) {
			printf("\t");
		}

		printf("0x%04" PRIx16 ",", data[i]);

		if (i % per_line == per_line - 1 || i + 1 == len) {
			printf("\n");
		} else {
			printf(" ");
		}
	}
	printf("};\n\n");
}

static void emit_u8_array(const char *name, const uint8_t *data, size_t len,
			  size_t per_line)
{
	printf("static const uint8_t %s[%zu] = {\n", name, len);
	for (size_t i = 0; i < len; ++i) {
		if (i % per_line == 0) {
			printf("\t");
		}

		printf("0x%02" PRIx8 ",", data[i]);

		if (i % per_line == per_line - 1 || i + 1 == len) {
			printf("\n");
		} else {
			printf(" ");
		}
	}
	printf("};\n\n");
}

int main(int argc, char **argv)
{
	if (argc < 2 || argv[1] == NULL || argv[1][0] == '\0') {
		fprintf(stderr,
			"usage: generate /path/to/DerivedCoreProperties.txt\n");
		exit(EXIT_FAILURE);
	}

	FILE *derived = fopen(argv[1], "r");
	if (derived == NULL) {
		fprintf(stderr, "failed to open DerivedCoreProperties.txt\n");
		exit(EXIT_FAILURE);
	}

	load_table(derived, all_table, sizeof(all_table));
	fclose(derived);
	memcpy(ascii_table, all_table, sizeof(ascii_table));

	size_t runs_len = 0;

	uint32_t end_cp = MAX_CODEPOINT + 1;
	uint32_t run_start = START_CODEPOINT;
	uint8_t current = all_table[START_CODEPOINT];
	for (uint32_t cp = START_CODEPOINT + 1; cp <= end_cp; ++cp) {
		uint8_t value = (cp <= MAX_CODEPOINT) ? all_table[cp] : 0;
		if (value != current) {
			if (runs_len >= MAX_RUNS) {
				bail("run table exceeds static capacity");
			}
			runs[runs_len++] = (struct run){run_start, current};
			run_start = cp;
			current = value;
		}
	}
	if (runs_len >= MAX_RUNS) {
		bail("run table exceeds static capacity");
	}
	runs[runs_len++] = (struct run){run_start, current};
	if (runs[runs_len - 1].start != end_cp) {
		if (runs_len >= MAX_RUNS) {
			bail("run table exceeds static capacity");
		}
		runs[runs_len++] = (struct run){end_cp, 0};
	}

	uint32_t block_count = (MAX_CODEPOINT >> SHIFT) + 1;
	uint32_t block_bits = bit_width(block_count - 1);
	if (block_bits <= TOP_BITS) {
		fprintf(stderr,
			"TOP_BITS (%u) must be smaller than block bit width (%u)\n",
			TOP_BITS, block_bits);
		exit(EXIT_FAILURE);
	}
	if (block_count > (sizeof(block_index) / sizeof(block_index[0]))) {
		bail("block index table too small");
	}
	if (block_count > (sizeof(block_to_leaf) / sizeof(block_to_leaf[0]))) {
		bail("block_to_leaf table too small");
	}
	uint32_t lower_bits = block_bits - TOP_BITS;
	uint32_t lower_size = 1u << lower_bits;
	uint32_t top_size = 1u << TOP_BITS;
	if (lower_size > 64) {
		bail("lower_size exceeds static buffer");
	}
	if (top_size > (sizeof(level1_table) / sizeof(level1_table[0]))) {
		bail("level1 table too small");
	}
	if (lower_size * top_size > MAX_LEVEL2_ENTRIES) {
		bail("level2 table exceeds static capacity");
	}

	size_t run_idx = 0;
	for (uint32_t block = 0; block < block_count; ++block) {
		uint32_t block_start = block << SHIFT;
		while (run_idx + 1 < runs_len
		       && runs[run_idx + 1].start <= block_start) {
			++run_idx;
		}
		block_index[block] = run_idx;
	}

	size_t leaf_runs_len = 0;
	size_t leaf_count = 0;
	size_t local_len = 0;

	for (uint32_t block = 0; block < block_count; ++block) {
		uint32_t block_start = block << SHIFT;
		uint32_t block_end = (block + 1) << SHIFT;
		if (block_end > MAX_CODEPOINT + 1) {
			block_end = MAX_CODEPOINT + 1;
		}

		local_len = 0;
		size_t idx = block_index[block];
		for (;;) {
			uint32_t start = runs[idx].start;
			uint8_t value = runs[idx].value;
			uint32_t next_start = runs[idx + 1].start;
			if (next_start <= block_start) {
				++idx;
				continue;
			}

			uint32_t run_from =
				(start < block_start) ? block_start : start;
			if (run_from < block_end) {
				if (local_len >= MAX_LOCAL_RUNS) {
					bail("local leaf run overflow");
				}
				local_runs[local_len++] = (struct leaf_run){
					(uint16_t)(run_from - block_start),
					value};
			}

			if (next_start >= block_end) {
				break;
			}
			++idx;
		}

		if (local_len >= MAX_LOCAL_RUNS) {
			bail("local leaf run overflow");
		}
		local_runs[local_len++] = (struct leaf_run){
			(uint16_t)(block_end - block_start), 0};

		size_t existing = SIZE_MAX;
		for (size_t i = 0; i < leaf_count; ++i) {
			if (leaves_equal(leaf_runs, leaves[i], local_runs,
					 local_len)) {
				existing = i;
				break;
			}
		}

		uint16_t leaf_id = 0;
		if (existing != SIZE_MAX) {
			leaf_id = (uint16_t)existing;
		} else {
			if (leaf_count >= MAX_UINT16) {
				bail("leaf count exceeds uint16");
			}
			if (leaf_count >= MAX_LEAVES) {
				bail("leaf table exceeds static capacity");
			}
			if (leaf_runs_len + local_len > MAX_LEAF_RUNS) {
				bail("leaf run table exceeds static capacity");
			}

			leaves[leaf_count] = (struct leaf_entry){
				(uint16_t)leaf_runs_len, (uint16_t)local_len};
			leaf_id = (uint16_t)leaf_count;
			++leaf_count;

			memcpy(leaf_runs + leaf_runs_len, local_runs,
			       local_len * sizeof(*local_runs));
			leaf_runs_len += local_len;
		}

		block_to_leaf[block] = leaf_id;
	}

	size_t level2_len = 0;
	size_t level2_count = 0;

	uint16_t level2_tmp[64] = {0}; /* lower_size max is 64 */

	for (uint32_t top = 0; top < top_size; ++top) {
		for (uint32_t low = 0; low < lower_size; ++low) {
			uint32_t block = top * lower_size + low;
			level2_tmp[low] = block_to_leaf[block];
		}

		size_t existing = SIZE_MAX;
		for (size_t i = 0; i < level2_count; ++i) {
			if (memcmp(level2_tables + i * lower_size, level2_tmp,
				   lower_size * sizeof(*level2_tmp))
			    == 0) {
				existing = i;
				break;
			}
		}

		uint16_t table_id = 0;
		if (existing != SIZE_MAX) {
			table_id = (uint16_t)existing;
		} else {
			if (level2_count >= MAX_UINT16) {
				fprintf(stderr,
					"level2 table count exceeds uint16: %zu\n",
					level2_count);
				exit(EXIT_FAILURE);
			}
			if (level2_len + lower_size > MAX_LEVEL2_ENTRIES) {
				bail("level2 table exceeds static capacity");
			}

			memcpy(level2_tables + level2_len, level2_tmp,
			       lower_size * sizeof(*level2_tmp));
			table_id = (uint16_t)level2_count;
			level2_len += lower_size;
			++level2_count;
		}

		level1_table[top] = table_id;
	}
	static uint16_t leaf_offsets[MAX_LEAVES + 1];
	static uint16_t leaf_run_starts[MAX_LEAF_RUNS];
	static uint8_t leaf_run_values[MAX_LEAF_RUNS];

	for (size_t i = 0; i < leaf_count; ++i) {
		leaf_offsets[i] = leaves[i].offset;
	}
	leaf_offsets[leaf_count] = (uint16_t)leaf_runs_len;

	for (size_t i = 0; i < leaf_runs_len; ++i) {
		leaf_run_starts[i] = leaf_runs[i].start;
		leaf_run_values[i] = leaf_runs[i].value;
	}

	printf("#ifndef UNICODE_DATA_H\n"
	       "#define UNICODE_DATA_H\n"
	       "// This file is autogenerated. DO NOT EDIT.\n"
	       "// This file is derived from the Unicode Character Database, and \n"
	       "// Is thus subject to the terms of the Unicode License V3.\n\n"
	       "#include <stddef.h>\n"
	       "#include <stdint.h>\n\n"
	       "enum {\n"
	       "\tSHIFT = %u,\n"
	       "\tSTART_CODEPOINT = %u,\n"
	       "\tBLOCK_COUNT = %u,\n"
	       "\tLOWER_BITS = %u,\n"
	       "\tLOWER_SIZE = %u,\n"
	       "};\n\n",
	       SHIFT, START_CODEPOINT, block_count, lower_bits, lower_size);

	emit_u8_array("ASCII_TABLE", ascii_table, START_CODEPOINT,
		      BYTE_VALUES_PER_LINE);
	emit_u16_array("LEAF_OFFSETS", leaf_offsets, leaf_count + 1,
		       INDEX_VALUES_PER_LINE);
	emit_u16_array("LEAF_RUN_STARTS", leaf_run_starts, leaf_runs_len,
		       INDEX_VALUES_PER_LINE);
	emit_u8_array("LEAF_RUN_VALUES", leaf_run_values, leaf_runs_len,
		      BYTE_VALUES_PER_LINE);
	emit_u16_array("LEVEL2_TABLES", level2_tables, level2_len,
		       INDEX_VALUES_PER_LINE);
	emit_u16_array("LEVEL1_TABLE", level1_table, top_size,
		       INDEX_VALUES_PER_LINE);

	printf("#endif // UNICODE_DATA_H\n");
	return 0;
}
