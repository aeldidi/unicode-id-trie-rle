package main

import (
	"bufio"
	"encoding/binary"
	"flag"
	"fmt"
	"log"
	"math/bits"
	"os"
	"regexp"
	"strconv"
	"strings"
)

const (
	maxCodepoint = 0x0fffff
	startCode    = 0x80
	shift        = 10
	topBits      = 6

	byteValuesPerLine  = 12
	indexValuesPerLine = 8
	maxUint16Value     = 1<<16 - 1
)

type run struct {
	start uint32
	value byte
}

type leafRun struct {
	start uint16
	value byte
}

func parseRange(field string) (uint32, uint32, error) {
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
		if err != nil {
			return 0, 0, err
		}
		return uint32(start), uint32(end), nil
	default:
		return 0, 0, fmt.Errorf("invalid range %q", field)
	}
}

func buildTable(path string) ([]byte, error) {
	file, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer file.Close()

	table := make([]byte, maxCodepoint+1)
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
		var bits byte
		if strings.Contains(prop, "XID_Start") {
			bits |= 1
		}
		if strings.Contains(prop, "XID_Continue") {
			bits |= 2
		}
		if bits == 0 {
			continue
		}

		start, end, err := parseRange(strings.TrimSpace(parts[0]))
		if err != nil {
			return nil, fmt.Errorf("parse range %q: %w", parts[0], err)
		}
		if start > maxCodepoint {
			continue
		}
		if end > maxCodepoint {
			end = maxCodepoint
		}

		for cp := start; cp <= end; cp++ {
			table[cp] |= bits
		}
	}
	if err := scanner.Err(); err != nil {
		return nil, err
	}

	return table, nil
}

func buildRuns(table []byte) []run {
	runs := make([]run, 0, 1024)
	endCP := uint32(maxCodepoint + 1)

	runStart := uint32(startCode)
	current := table[startCode]
	for cp := uint32(startCode + 1); cp <= endCP; cp++ {
		value := byte(0)
		if cp <= maxCodepoint {
			value = table[cp]
		}

		if value != current {
			runs = append(runs, run{start: runStart, value: current})
			runStart = cp
			current = value
		}
	}
	runs = append(runs, run{start: runStart, value: current})
	if runs[len(runs)-1].start != endCP {
		runs = append(runs, run{start: endCP, value: 0})
	}

	return runs
}

func buildBlockIndex(runs []run, blockCount int) []int {
	index := make([]int, blockCount)
	runIdx := 0
	for block := 0; block < blockCount; block++ {
		blockStart := uint32(block << shift)
		for runIdx+1 < len(runs) && runs[runIdx+1].start <= blockStart {
			runIdx++
		}
		index[block] = runIdx
	}
	return index
}

func serializeLeafRuns(runs []leafRun) string {
	buf := make([]byte, 0, len(runs)*3)
	for _, r := range runs {
		buf = binary.LittleEndian.AppendUint16(buf, r.start)
		buf = append(buf, r.value)
	}
	return string(buf)
}

func serializeUint16s(vals []uint16) string {
	buf := make([]byte, 0, len(vals)*2)
	for _, v := range vals {
		buf = binary.LittleEndian.AppendUint16(buf, v)
	}
	return string(buf)
}

func buildLeaves(runs []run, blockIndex []int, blockCount int) ([]leafRun, []uint16, []uint16) {
	leafRuns := make([]leafRun, 0, 4096)
	leafOffsets := make([]uint16, 0, 128)
	blockToLeaf := make([]uint16, 0, blockCount)
	leafMap := make(map[string]uint16)

	for block := 0; block < blockCount; block++ {
		blockStart := uint32(block << shift)
		blockEnd := uint32((block + 1) << shift)
		if blockEnd > uint32(maxCodepoint+1) {
			blockEnd = uint32(maxCodepoint + 1)
		}

		idx := blockIndex[block]
		local := make([]leafRun, 0, 8)
		for {
			start := runs[idx].start
			value := runs[idx].value
			nextStart := runs[idx+1].start
			if nextStart <= blockStart {
				idx++
				continue
			}

			runFrom := start
			if runFrom < blockStart {
				runFrom = blockStart
			}
			if runFrom < blockEnd {
				local = append(local, leafRun{
					start: uint16(runFrom - blockStart),
					value: value,
				})
			}

			if nextStart >= blockEnd {
				break
			}
			idx++
		}

		local = append(local, leafRun{
			start: uint16(blockEnd - blockStart),
			value: 0,
		})

		key := serializeLeafRuns(local)
		leafID, ok := leafMap[key]
		if !ok {
			if len(leafMap) >= maxUint16Value {
				log.Fatalf("leaf count exceeds uint16: %d", len(leafMap))
			}
			if len(leafRuns)+len(local) > maxUint16Value {
				log.Fatalf("leaf run table exceeds uint16: %d", len(leafRuns)+len(local))
			}
			leafID = uint16(len(leafMap))
			leafOffsets = append(leafOffsets, uint16(len(leafRuns)))
			leafRuns = append(leafRuns, local...)
			leafMap[key] = leafID
		}

		blockToLeaf = append(blockToLeaf, leafID)
	}

	if len(leafRuns) > maxUint16Value {
		log.Fatalf("leaf run table exceeds uint16: %d", len(leafRuns))
	}
	leafOffsets = append(leafOffsets, uint16(len(leafRuns)))
	return leafRuns, leafOffsets, blockToLeaf
}

func buildLevelTables(blockToLeaf []uint16, lowerSize, topSize int) ([]uint16, []uint16) {
	level2Map := make(map[string]uint16)
	level2Tables := make([]uint16, 0, lowerSize)
	level1Table := make([]uint16, 0, topSize)

	for top := 0; top < topSize; top++ {
		table := make([]uint16, lowerSize)
		for low := 0; low < lowerSize; low++ {
			block := top*lowerSize + low
			table[low] = blockToLeaf[block]
		}

		key := serializeUint16s(table)
		tableID, ok := level2Map[key]
		if !ok {
			if len(level2Map) >= maxUint16Value {
				log.Fatalf("level2 table count exceeds uint16: %d", len(level2Map))
			}
			tableID = uint16(len(level2Map))
			level2Map[key] = tableID
			level2Tables = append(level2Tables, table...)
		}

		level1Table = append(level1Table, tableID)
	}

	return level2Tables, level1Table
}

func splitLeafRuns(runs []leafRun) ([]uint16, []byte) {
	offsets := make([]uint16, len(runs))
	values := make([]byte, len(runs))
	for i, r := range runs {
		offsets[i] = r.start
		values[i] = r.value
	}
	return offsets, values
}

func emitUint16Array(w *bufio.Writer, name string, data []uint16, perLine int) {
	fmt.Fprintf(w, "var %s = [...]uint16{\n", name)
	for i, v := range data {
		if i%perLine == 0 {
			fmt.Fprint(w, "\t")
		}
		fmt.Fprintf(w, "0x%04x,", v)
		if i%perLine == perLine-1 || i+1 == len(data) {
			fmt.Fprintln(w)
		} else {
			fmt.Fprint(w, " ")
		}
	}
	fmt.Fprintln(w, "}")
	fmt.Fprintln(w)
}

func emitClassArray(w *bufio.Writer, name string, data []byte, perLine int) {
	fmt.Fprintf(w, "var %s = [...]IdentifierClass{\n", name)
	for i, v := range data {
		if i%perLine == 0 {
			fmt.Fprint(w, "\t")
		}
		fmt.Fprintf(w, "0x%02x,", byte(v))
		if i%perLine == perLine-1 || i+1 == len(data) {
			fmt.Fprintln(w)
		} else {
			fmt.Fprint(w, " ")
		}
	}
	fmt.Fprintln(w, "}")
	fmt.Fprintln(w)
}

func main() {
	log.SetFlags(0)
	log.SetPrefix("generate: ")
	input := flag.String("i", "", "the path to DerivedCoreProperties.txt")
	output := flag.String("o", "", "the path to the output file")
	flag.Parse()

	if *input == "" {
		log.Fatal("must provide input file with -i")
	}
	if *output == "" {
		log.Fatal("must provide output file with -o")
	}

	pkg := os.Getenv("GOPACKAGE")
	if pkg == "" {
		log.Fatal("GOPACKAGE not set - run this tool with go generate")
	}

	table, err := buildTable(*input)
	if err != nil {
		log.Fatalf("failed to build table: %v", err)
	}

	runs := buildRuns(table)
	if len(runs) >= 1<<16 {
		log.Fatalf("run table too large for uint16 index: %d", len(runs))
	}

	blockCount := (maxCodepoint >> shift) + 1
	blockIndex := buildBlockIndex(runs, blockCount)
	blockBits := 32 - bits.LeadingZeros32(uint32(blockCount-1))
	if blockBits <= topBits {
		log.Fatalf("topBits (%d) must be smaller than block bit width (%d)", topBits, blockBits)
	}
	lowerBits := blockBits - topBits
	lowerSize := 1 << lowerBits
	topSize := 1 << topBits

	leafRuns, leafOffsets, blockToLeaf := buildLeaves(runs, blockIndex, blockCount)
	leafRunStarts, leafRunValues := splitLeafRuns(leafRuns)
	level2Tables, level1Table := buildLevelTables(blockToLeaf, lowerSize, topSize)

	out, err := os.Create(*output)
	if err != nil {
		log.Fatal(err)
	}
	defer out.Close()

	writer := bufio.NewWriter(out)
	defer writer.Flush()

	fmt.Fprintf(writer, "// Code generated by \"generate %s\"; DO NOT EDIT.\n", strings.Join(os.Args[1:], " "))
	fmt.Fprintf(writer, "package %s\n\n", pkg)
	fmt.Fprintln(writer, "const (")
	fmt.Fprintf(writer, "\tshift = %d\n", shift)
	fmt.Fprintf(writer, "\tblockCount = %d\n", blockCount)
	fmt.Fprintf(writer, "\tlowerBits = %d\n", lowerBits)
	fmt.Fprintf(writer, "\tlowerSize = %d\n", lowerSize)
	fmt.Fprintln(writer, ")")
	fmt.Fprintln(writer)

	emitUint16Array(writer, "leafOffsets", leafOffsets, indexValuesPerLine)
	emitUint16Array(writer, "leafRunStarts", leafRunStarts, indexValuesPerLine)
	emitClassArray(writer, "leafRunValues", leafRunValues, byteValuesPerLine)
	emitUint16Array(writer, "level2Tables", level2Tables, indexValuesPerLine)
	emitUint16Array(writer, "level1Table", level1Table, indexValuesPerLine)
}
