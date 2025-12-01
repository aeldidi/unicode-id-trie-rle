A Go port of the trie-based implementation.

Run `go generate ./...` to rebuild `ident_generated.go` from the repo's
`DerivedCoreProperties.txt`, then drop `ident.go` plus the generated file
wherever you need it.

`go test ./...` re-parses the derived data, computes the reference
start/continue bits for every scalar value, and fails if
`unicodeIdentifierClass` disagrees. Point `GOCACHE` at a writable
directory if your environment doesn't already have one.
