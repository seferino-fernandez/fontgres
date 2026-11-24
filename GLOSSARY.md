# fontgres

fontgres finds the fonts installed on a machine and answers CSS-style font queries about them, as a library, a command-line tool and an MCP server.

## Language

**Face**:
One style of a typeface as a font file holds it, such as Helvetica Bold. A query returns a face.
_Avoid_: Font, variant

**Font file**:
A file on disk that holds one or more faces. It is identified by where it is, so a copy or a hard link is a different font file, and a symbolic link is the same one.
_Avoid_: Font

**Collection**:
A font file that holds more than one face.

**Named font file**:
A font file the user asked for by path, as opposed to one found in a directory.
_Avoid_: Explicit file, extra file

**Family**:
The faces that share a family name. A face can carry its family name in several languages.
_Avoid_: Typeface, font

**Primary family name**:
The one name a family is listed under: the English (United States) name when the font has one.

**Generic family**:
One of the CSS names serif, sans-serif, cursive, fantasy and monospace, each standing for one family chosen per database.

**Query**:
A request for one face: families in priority order, and the weight, style and stretch wanted.

**System fonts**:
The fonts a platform installs or lets its users install, found in the directories that platform's rules name.

**Load order**:
The order in which faces entered a database. Within a directory it follows file names, and within a collection it follows the faces' positions in the file.
_Avoid_: Insertion order, iteration order

**Font load report**:
What one load tells its caller: the faces it added, how many font files it skipped as already loaded, and which font files or faces failed and why.
_Avoid_: Load result, load summary

**Tie**:
Faces of one family that are equal in stretch, style and weight. A query answers a tie with the face earliest in load order.
