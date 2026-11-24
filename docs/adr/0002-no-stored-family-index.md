# No stored family index

Finding a family's faces scans every face; the database keeps no index from family name to faces. We measured both on a Mac with 1,104 faces: a scan takes about 2.5 µs per family name tried, an index takes about 88 µs to build and 0.03 µs per lookup, so the index pays for itself only after about 35 lookups. A command-line run makes one query, which makes the index a net cost there, and a long-running server does not notice the difference.

The scan lives in one place, `Database::family_faces`, which the query, the generic-family resolution and the listings share.

## Consequences

Revisit this if a database is expected to hold tens of thousands of faces, or if one process starts making thousands of queries per second.
