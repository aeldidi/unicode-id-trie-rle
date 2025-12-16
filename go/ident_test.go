package unicode_id_trie_rle

import (
	"bufio"
	"os"
	"path/filepath"
	"regexp"
	"runtime"
	"strconv"
	"strings"
	"testing"
)

const maxScalar = 0x110000

func derivedDataPath(t *testing.T) string {
	t.Helper()
	_, filename, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatalf("runtime.Caller failed")
	}

	repoRoot := filepath.Clean(filepath.Join(filepath.Dir(filename), ".."))
	path := filepath.Join(repoRoot, "DerivedCoreProperties.txt")
	if _, err := os.Stat(path); err != nil {
		t.Fatalf("failed to stat derived data at %q: %v", path, err)
	}
	return path
}

func parseRangeField(field string) (uint32, uint32, error) {
	parts := strings.Split(field, "..")
	switch len(parts) {
	case 1:
		v, err := strconv.ParseUint(strings.TrimSpace(parts[0]), 16, 32)
		return uint32(v), uint32(v), err
	case 2:
		start, err := strconv.ParseUint(strings.TrimSpace(parts[0]), 16, 32)
		if err != nil {
			return 0, 0, err
		}
		end, err := strconv.ParseUint(strings.TrimSpace(parts[1]), 16, 32)
		return uint32(start), uint32(end), err
	default:
		return 0, 0, strconv.ErrSyntax
	}
}

func derivedIdentifierTable(t *testing.T) []IdentifierClass {
	t.Helper()

	path := derivedDataPath(t)
	file, err := os.Open(path)
	if err != nil {
		t.Fatalf("failed to open %q: %v", path, err)
	}
	defer file.Close()

	table := make([]IdentifierClass, maxScalar)
	scanner := bufio.NewScanner(file)
	scanner.Buffer(make([]byte, 0, 64*1024), 1<<20)
	commentRe := regexp.MustCompile(`#.*`)
	for scanner.Scan() {
		line := commentRe.ReplaceAllString(scanner.Text(), "")
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}

		parts := strings.SplitN(line, ";", 2)
		if len(parts) != 2 {
			continue
		}

		prop := strings.TrimSpace(parts[1])
		var bits IdentifierClass
		if strings.Contains(prop, "XID_Start") {
			bits |= Start
		}
		if strings.Contains(prop, "XID_Continue") {
			bits |= Continue
		}
		if bits == 0 {
			continue
		}

		start, end, err := parseRangeField(strings.TrimSpace(parts[0]))
		if err != nil {
			t.Fatalf("failed to parse %q: %v", parts[0], err)
		}
		if start >= maxScalar {
			continue
		}
		if end >= maxScalar {
			end = maxScalar - 1
		}

		for cp := start; cp <= end; cp++ {
			table[int(cp)] |= bits
		}
	}

	if err := scanner.Err(); err != nil {
		t.Fatalf("failed to read %q: %v", path, err)
	}

	return table
}

func TestUnicodeIdentifierClassMatchesDerivedData(t *testing.T) {
	table := derivedIdentifierTable(t)
	for cp := rune(0); cp <= 0x10ffff; cp++ {
		expected := table[int(cp)]
		if cp >= 0x100000 {
			if expected != Other {
				t.Fatalf("derived data marks unsupported codepoint U+%04X", cp)
			}
			expected = Other
		}

		class := UnicodeIdentifierClass(cp)
		if class != expected {
			t.Fatalf("unicodeIdentifierClass mismatch at U+%04X: expected %d, got %d", cp, expected, class)
		}
	}
}
