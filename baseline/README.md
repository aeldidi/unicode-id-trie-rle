A baseline implementation. All this does is output a packed table of 2 bit
values, with the least significant bit set for `*_Start` and the most
significant of the two set for `*_Continue`.

The lookup function looks up the value directly at the bit index.
