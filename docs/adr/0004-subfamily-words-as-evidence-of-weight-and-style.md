# The subfamily name is the last evidence of weight and style

A face's weight and style come from its tables first: the `OS/2` table, then for style the italic angle in `post`, and for a face without `OS/2` the bold and italic flags in `head`. When the tables leave a face at weight 400, the whole word Bold in its English subfamily name makes it 700. When they leave it at normal style, the whole word Italic or Oblique gives it that style.

We added the words because the tables of some installed fonts are wrong. On a Mac with 1,104 faces, 18 faces named Bold declare weight 400, among them the Bold of Bangla MN and of twelve more "MN" and "Sangam MN" families, so a query for Bangla MN at weight 700 returned the regular face. Four faces named Italic, such as Helvetica Neue Thin Italic, declare a normal style.

The words never override a table that says something else. A face that declares 500 and is named Bold stays at 500, and a face that declares oblique and is named Italic stays oblique. Only whole words count, so Semibold is not read as Bold.

## Considered Options

- **Follow fontconfig.** For style, fontconfig reads Italic and Oblique from the style name first and looks at the font's italic flag only when neither word is there, so the name wins. We read the words last: a table that says italic or oblique is kept, and a word only fills in for tables that say normal. For weight, fontconfig reads the style name only when the font has no `OS/2` table, so it would leave those 18 faces at 400. We go further there because a query that asks for bold should find the face named Bold.
- **Read every weight word**, such as Light, Medium and Black. Seven faces here carry such a word at a declared 400. We left them alone: mapping each word to a number is a larger table of guesses than the one word a query for bold needs.
- **Let the name override the tables.** The tables are right for most fonts, so a name would then break faces that are read correctly today.

## Consequences

A face named with two words, such as Semi Bold or Extra Bold, that declares 400 is read as 700, not 600 or 800. No font here does that.

A face named Oblique whose `post` table gives an italic angle is italic, not oblique, because the angle is read before the name. Fourteen faces here are read that way, as they were before this decision; fontconfig would call them oblique.

Words in other languages, such as Fett or Gras, are not read, and neither is Inclined.
