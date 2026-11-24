# No stored face index

Every run reads the font files again; nothing about their faces is kept on disk between runs. We measured a command-line query on a Mac with 1,104 faces in 687 font files and a warm file cache: the run takes about 6 ms, about 1.4 ms of it process start, so a stored index could save at most about 5 ms. Against that, an index can go stale, which is a kind of defect the database does not have today, and checking 687 font files for changes has its own cost, which we did not measure. The MCP server loads once per session and would gain nothing.

## Considered Options

- **Store the faces of each font file and parse only the font files that changed.** The interface would stay the same and the load would stop growing with the number of font files, but every run would depend on the freshness check being right.

## Consequences

Revisit this if a run with a cold file cache, or on Linux or Windows, shows the load taking tens of milliseconds. None of those was measured.
