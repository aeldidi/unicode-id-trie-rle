A Go port of the trie-based implementation.

To use it from another module, add a dependency on
`github.com/aeldidi/unicode-id-trie-rle/go` (Go 1.22+):

```sh
go get github.com/aeldidi/unicode-id-trie-rle/go@latest
```

Then import the package and call `IsIdent` or `UnicodeIdentifierClass`:

```go
package main

import "github.com/aeldidi/unicode-id-trie-rle/go"

func main() {
	if unicode_id_trie_rle.IsIdent([]rune("id_42")) {
		// ...
	}
}
```

Run `go generate ./...` to rebuild `ident_generated.go` from the repo's
`DerivedCoreProperties.txt`, then drop `ident.go` plus the generated file
wherever you need it.

`go test ./...` re-parses the derived data, computes the reference
start/continue bits for every scalar value, and fails if
`unicodeIdentifierClass` disagrees.
