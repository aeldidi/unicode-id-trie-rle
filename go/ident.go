//go:generate go run unicode_idents/go/generate -i ../DerivedCoreProperties.txt -o ident_generated.go
package main

import "sort"

type IdentifierClass byte

const (
	Other IdentifierClass = iota
	Start
	Continue
)

const (
	startCodepoint = 0x80
	blockMask      = (1 << shift) - 1
	lowerMask      = (1 << lowerBits) - 1
)

type leaf struct {
	offset uint16
	len    uint16
}

var asciiTable = func() [startCodepoint]IdentifierClass {
	var table [startCodepoint]IdentifierClass
	for c := byte('A'); c <= byte('Z'); c++ {
		table[c] = Start | Continue
	}
	for c := byte('a'); c <= byte('z'); c++ {
		table[c] = Start | Continue
	}
	for c := byte('0'); c <= byte('9'); c++ {
		table[c] = Continue
	}
	table['_'] = Continue
	return table
}()

func loadLeaf(idx uint16) leaf {
	start := leafOffsets[idx]
	end := leafOffsets[idx+1]
	return leaf{offset: start, len: end - start}
}

func leafValue(l leaf, offset uint16) IdentifierClass {
	start := int(l.offset)
	end := start + int(l.len)
	runs := leafRunStarts[start:end]
	values := leafRunValues[start:end]

	idx := sort.Search(len(runs), func(i int) bool {
		return runs[i] > offset
	})
	if idx == 0 {
		return values[0]
	}
	return values[idx-1]
}

func UnicodeIdentifierClass(cp rune) IdentifierClass {
	if cp < 0 {
		return Other
	}
	if cp < startCodepoint {
		return asciiTable[cp]
	}
	if cp >= 0x100000 {
		return Other
	}

	block := uint32(cp) >> shift
	top := block >> lowerBits
	bottom := block & lowerMask
	level2Idx := level1Table[top]
	leafIdx := level2Tables[int(level2Idx)*lowerSize+int(bottom)]
	l := loadLeaf(leafIdx)
	offset := uint16(uint32(cp) & blockMask)
	return leafValue(l, offset)
}

// U+200C ZERO WIDTH NON-JOINER and U+200D ZERO WIDTH JOINER are
// allowed *inside* an identifier (never first or last).
const (
	ZWNJ = 0x200c
	ZWJ  = 0x200d
)

func IsIdent(s []rune) bool {
	if len(s) == 0 {
		return false
	}

	if (UnicodeIdentifierClass(s[0]) & Start) == 0 {
		return false
	}

	for i, c := range s[1:] {
		p := UnicodeIdentifierClass(c)
		if p&Continue == 0 {
			// the two special characters are only allowed in the
			// middle, not the end.
			if (c != ZWNJ && c != ZWJ) || i+1 == len(s) {
				return false
			}
		}
	}
	return true
}
