A C port of the trie-based implementation.

To use, compile `generate.c` into a binary with
`cc -std=c11 -o generate generate.c`, then run it as
`./generate path/to/DerivedCoreProperties.txt > unicode_data.h` to create the
file `unicode_data.h`. Then simply copy that file along with
`unicode_identifiers.c` into your project to use. I would recommend also
copying the `generate.c` script, since it allows you to re-generate
`unicode_data.h` when new Unicode versions come out.

There's a quick check in `unicode_identifiers_test.c` which pulls in the actual
`unicode_data.h` that `generate.c` spits out, re-parses
`DerivedCoreProperties.txt`, and then walks every codepoint to make sure
`unicode_identifier_class` agrees with the derived data. Build it with
`cc -o unicode_identifiers_test -std=c11 unicode_identifiers_test.c` (after
running `./generate` so that `unicode_data.h` exists) and run
`./unicode_identifiers_test path/to/DerivedCoreProperties.txt` (or set the
`DERIVED_CORE_PROPERTIES` env var) whenever you tweak the code or refresh the
data file.
