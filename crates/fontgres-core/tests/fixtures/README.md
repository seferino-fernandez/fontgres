# Test fixtures

Fonts that `tests/fonts` must not hold: the CLI and MCP tests load that whole
directory and expect only Tuffy.

## `TuffyMissingPS.ttf`

Tuffy (`../fonts/Tuffy.ttf`, public domain, see `../fonts/LICENSE.txt`) with no
PostScript name, for the fallback in `postscript_name.rs`. Made with fontTools'
`ttx`:

1. `ttx -o TuffyMissingPS.ttx ../fonts/Tuffy.ttf`
2. In the `name` table, delete every `nameID="6"` record, and in every
   `nameID="1"` and `nameID="4"` record replace `Tuffy` with
   `Tuffy Missing PS` (both the Macintosh and the Windows records).
3. `ttx -o TuffyMissingPS.ttf TuffyMissingPS.ttx`

The new family name makes the generated PostScript name, `Tuffy-Missing-PS`,
differ from anything the original font carries.
