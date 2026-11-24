# The font database is append-only

A `Database` only gains faces: there is no way to remove one. fontdb, which this crate forks, had `remove_face`, and we deleted it because nothing in the workspace called it and it was the only thing keeping load order from being the storage order. Without it, faces sit in one list in load order, an ID is a face's position in that list, and a tie between faces always goes to the one loaded first.

A caller who wants fewer faces builds a new database. Reloading a font file already required that, because a database remembers every font file it has loaded.

## Considered Options

- **Keep `remove_face` over storage that never reuses an ID.** This keeps load order stable too, but it leaves holes in the list, so `faces()` could not be a slice and an ID could not be a position.
- **Keep the slot map.** A slot map reuses the slot freed last, so after a removal a face loaded later could win a tie over one loaded earlier.
