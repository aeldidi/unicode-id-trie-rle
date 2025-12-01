#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include "unicode_data.h"

enum unicode_identifier_property {
	IDENTIFIER_OTHER = 0,
	IDENTIFIER_START = 1,
	IDENTIFIER_CONTINUE = 2,
};

#define BLOCK_MASK ((1u << SHIFT) - 1u)
#define LOWER_MASK ((1u << LOWER_BITS) - 1u)

struct leaf {
	uint16_t offset;
	uint16_t len;
};

static inline struct leaf load_leaf(uint16_t idx)
{
	uint16_t start = LEAF_OFFSETS[idx];
	uint16_t end = LEAF_OFFSETS[idx + 1];
	return (struct leaf){start, (uint16_t)(end - start)};
}

static uint8_t leaf_value(struct leaf leaf, uint16_t offset)
{
	const uint16_t *runs = &LEAF_RUN_STARTS[leaf.offset];
	const uint8_t *values = &LEAF_RUN_VALUES[leaf.offset];

	size_t lo = 0;
	size_t hi = leaf.len;
	while (lo < hi) {
		size_t mid = lo + (hi - lo) / 2;
		if (runs[mid] > offset) {
			hi = mid;
		} else {
			lo = mid + 1;
		}
	}

	size_t idx = (lo == 0) ? 0 : lo - 1;
	return values[idx];
}

static inline uint8_t ascii_class(uint32_t cp)
{
	return ASCII_TABLE[cp];
}

uint8_t unicode_identifier_class(uint32_t cp)
{
	if (cp < START_CODEPOINT) {
		return ascii_class(cp);
	}

	if (cp >= 0x100000) {
		return IDENTIFIER_OTHER;
	}

	uint32_t block = cp >> SHIFT;
	uint32_t top = block >> LOWER_BITS;
	uint32_t bottom = block & LOWER_MASK;
	uint16_t level2_idx = LEVEL1_TABLE[top];
	uint16_t leaf_idx =
		LEVEL2_TABLES[(size_t)level2_idx * LOWER_SIZE + bottom];
	struct leaf leaf = load_leaf(leaf_idx);
	uint16_t offset = (uint16_t)(cp & BLOCK_MASK);
	return leaf_value(leaf, offset);
}

// U+200C ZERO WIDTH NON-JOINER and U+200D ZERO WIDTH JOINER are
// allowed *inside* an identifier (never first or last).
#define ZWNJ 0x200c
#define ZWJ 0x200d

// Returns true if the given array of codepoints is a valid Unicode identifier,
// defined by Unicode Standard Annex #31.
bool unicode_is_identifier(const uint32_t *cp, size_t len)
{
	if (len == 0) {
		return false;
	}

	if ((unicode_identifier_class(cp[0]) & IDENTIFIER_START) == 0) {
		return false;
	}

	for (size_t i = 1; i < len; ++i) {
		uint32_t c = cp[i];
		uint8_t p = unicode_identifier_class(c);
		if ((p & IDENTIFIER_CONTINUE) == 0) {
			// the two special characters are only allowed in the
			// middle, not the end.
			if ((c != ZWNJ && c != ZWJ) || i + 1 == len) {
				return false;
			}
		}
	}
	return true;
}
