# fontgres MCP Server

This server answers questions about the fonts it loaded at startup: the
system's installed fonts and any configured font directories and files.

- `fontgres_list_faces`: list every face (family, PostScript name, weight,
  style, stretch, monospaced, file path); `family` narrows it to one family.
- `fontgres_list_families`: list the distinct family names.
- `fontgres_query_font`: find the face that best matches `families` (in
  priority order; CSS generic names such as `sans-serif` work), `weight`
  (1–1000), `style` and `stretch`, as CSS font matching does.

Family names in a query match exactly, case included, so look names up with
the family list first. Every tool is read-only and rejects an argument it does
not declare, so send only the listed arguments.
