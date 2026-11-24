//! An in-memory font database with CSS-like queries, and the configuration and
//! font views the `fontgres` CLI and MCP server share.
//!
//! Load fonts from raw data, files, directories or the system's font
//! directories into a [`Database`], then list its faces or ask it for the
//! face that best matches a [`Query`], using the CSS font matching algorithm
//! for stretch, style and weight. The crate is a fork of
//! [fontdb](https://github.com/RazrFalcon/fontdb) 0.24.0. The database at the
//! crate root began as fontdb's and has since diverged: it only gains faces,
//! keeps them in load order, and a query returns the face itself.
//!
//! The database only stores and matches faces. It reads a face's names, style,
//! weight, stretch and monospace flag, and leaves other font properties to a
//! font parser such as [ttf-parser]. It has no font fallback, though it can be
//! the storage for one, and no global instance: the caller decides where a
//! [`Database`] lives. Only TrueType and OpenType fonts (`.ttf`, `.otf`,
//! `.ttc`, `.otc`) load.
//!
//! # Fonts and faces
//!
//! A font file holds one or more font faces. A plain `.ttf` or `.otf` file
//! usually holds one face, while a collection (`.ttc`) can hold many. The
//! database stores faces: loading a collection of five faces adds five
//! [`FaceInfo`] entries, each with its own [`ID`], all pointing at the same
//! [`Source`].
//!
//! # Quick start
//!
//! ```
//! use fontgres_core::{Database, Family, GenericFamily, Query, Weight};
//!
//! # let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fonts/Tuffy.ttf");
//! let mut db = Database::new();
//! let report = db.load_font_data(std::fs::read(path)?);
//! assert!(report.failures().is_empty());
//!
//! let query = Query {
//!     families: &[
//!         Family::Name("Tuffy"),
//!         Family::Generic(GenericFamily::SansSerif),
//!     ],
//!     weight: Weight::BOLD,
//!     ..Query::default()
//! };
//! let face = db.query(&query).expect("Tuffy is loaded");
//! assert_eq!(face.post_script_name, "Tuffy");
//! // Tuffy has no bold face, so the query falls back to its medium one.
//! assert_eq!(face.weight, Weight::MEDIUM);
//! # Ok::<(), std::io::Error>(())
//! ```
//!
//! To load the fonts installed on the machine, call
//! `Database::load_system_fonts` (the `fs` feature) instead.
//!
//! # Feature flags
//!
//! - `std` (default): links the standard library. Without it the crate is
//!   `no_std` with `alloc`, and fonts load only from in-memory data.
//! - `fs` (default): loading fonts from files, directories and the system's
//!   font directories, and the `env` module.
//! - `memmap` (default): memory-maps font files while reading them instead of
//!   copying them into memory.
//! - `fontconfig` (default): on Linux and other Unix systems except macOS,
//!   iOS and Android, reads the font directories and generic families from
//!   fontconfig's configuration. Without it, system fonts load from a fixed
//!   list of directories, which misses the fonts on systems such as NixOS.
//! - `cli`: the `config`, `models`, `error` and `cli` modules, which the
//!   `fontgres` binaries share, and the typed parsers of the `env` module.
//! - `schema`: derives `schemars::JsonSchema` on the `models` views. Implies
//!   `cli`.
//!
//! # Failures and logging
//!
//! One bad font does not fail a load. A font file that cannot be read, and a
//! face that fails to parse, is skipped and listed in the [`FontLoadReport`]
//! the load returns, with the reason. The crate does not log them.
//!
//! Through the [`log`](https://docs.rs/log) crate it logs only what is not a
//! font's failure: a warning when fontconfig's configuration lists no
//! directory, so the fixed directories load instead, and at debug level a
//! fontconfig file it skipped and a file it could not mark for random access.
//!
//! # Memory-mapped files
//!
//! With the `memmap` feature, which is on by default, a font file is
//! memory-mapped while a load or a [`Database::with_face_data`] call reads it,
//! and the mapping is dropped before the call returns. If the file is truncated
//! or rewritten in place during that call, the behavior is undefined, and in
//! practice the process can crash. That holds whoever changes the file: another
//! process, another thread, a `log` backend, or the closure passed to
//! [`Database::with_face_data`]. The crate does not prevent it, so these safe
//! functions are unsound when a file changes under them. Build without
//! `memmap` to read files into memory instead.
//!
//! [ttf-parser]: https://github.com/harfbuzz/ttf-parser

#![cfg_attr(not(feature = "std"), no_std)]
// The module map links to items that exist only under the `cli` feature, so
// it is part of the crate docs only when they are.
#![cfg_attr(
    feature = "cli",
    doc = "\n# Modules\n\n\
- [`config`]: layered configuration (CLI flags, `FONTGRES_*` environment \
variables, `config.toml`, defaults) for which fonts to load and how to log.\n\
- [`models`]: [`FontsConfig::build_database`], the serializable \
[`models::Face`] view, [`models::FontQuery`] and the face and family \
listings.\n\
- [`env`](mod@env): the environment-variable source the configuration reads \
through, and its typed parsers.\n\
- [`error`]: [`Error`] and [`ConfigError`].\n\
- [`cli`]: the `completions` and `man` subcommand helpers.\n"
)]

extern crate alloc;

mod ttf_parser;

#[cfg(not(feature = "std"))]
use alloc::{string::String, vec::Vec};

pub use generic_family::{GenericFamilies, GenericFamily};
pub use ttf_parser::Language;
pub use ttf_parser::Width as Stretch;

/// Identifies a face within one [`Database`]: the face's position in the
/// database's load order.
///
/// IDs ascend in load order, so comparing two IDs of one database tells which
/// face was loaded first, and the faces one load adds have consecutive IDs. A
/// database never removes a face, so an ID it issued stays valid and is never
/// given to another face.
///
/// An ID carries no reference to the database that issued it. Looking it up
/// in another database returns `None` or, if that database holds a face at
/// the same position, an unrelated face; keeping IDs with their database is
/// up to the caller.
///
/// `Display` writes unequal IDs as different strings; the format is not
/// otherwise specified.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd, Debug)]
pub struct ID(usize);

impl ID {
    /// Returns a placeholder ID that matches no face.
    ///
    /// Use it to fill [`FaceInfo::id`] before passing the face to
    /// [`Database::push_face_info`], which replaces it.
    #[inline]
    pub fn dummy() -> Self {
        // A database cannot hold this many faces: a `Vec` holds at most
        // `isize::MAX` bytes, and a face takes more than one.
        Self(usize::MAX)
    }
}

impl core::fmt::Display for ID {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// What one load did: the faces it added, the font files it skipped, and
/// what it could not load.
///
/// Every `load_*` method of [`Database`] returns one. A load reports its
/// failures only here, so a caller that drops the report never learns of
/// them.
///
/// # Examples
///
/// ```
/// use fontgres_core::{Database, FontLoadFailureReason};
///
/// # let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fonts/Tuffy.ttf");
/// let mut db = Database::new();
/// let report = db.load_font_data(std::fs::read(path)?);
/// assert_eq!(report.added().len(), 1);
/// assert!(report.failures().is_empty());
///
/// let report = db.load_font_data(b"not a font".to_vec());
/// assert_eq!(report.added().len(), 0);
/// let [failure] = report.failures() else { panic!("one face fails") };
/// assert!(matches!(failure.reason, FontLoadFailureReason::MalformedFont));
/// assert_eq!(failure.to_string(), "cannot load face 0: malformed font");
/// # Ok::<(), std::io::Error>(())
/// ```
#[derive(Debug)]
#[must_use = "a load reports the fonts it could not load only here"]
pub struct FontLoadReport {
    /// The positions in load order of the faces the load added.
    added: core::ops::Range<usize>,
    skipped: usize,
    failures: Vec<FontLoadFailure>,
}

impl FontLoadReport {
    /// Returns the IDs of the faces the load added, in load order.
    ///
    /// The IDs are consecutive: no other face sits between two faces of one
    /// load.
    pub fn added(&self) -> impl DoubleEndedIterator<Item = ID> + ExactSizeIterator + use<> {
        self.added.clone().map(ID)
    }

    /// Returns how many font files the load reached and skipped as already
    /// loaded.
    ///
    /// Only the file-system loads skip files; [`Database`] documents which.
    /// A load from memory always returns zero.
    pub fn skipped(&self) -> usize {
        self.skipped
    }

    /// Returns each font file and face the load could not add, in the order
    /// the load reached them.
    pub fn failures(&self) -> &[FontLoadFailure] {
        &self.failures
    }

    /// Returns the failures by value, as [`failures`](Self::failures) orders
    /// them.
    pub fn into_failures(self) -> Vec<FontLoadFailure> {
        self.failures
    }
}

/// One font file, or one face of a font, that a load could not add; part of
/// a [`FontLoadReport`].
///
/// `Display` writes the failure on one line, naming the face, the file and
/// the reason, such as `cannot load face 2 of /fonts/a.ttc: malformed font`.
///
/// The struct is non-exhaustive: read its fields, and expect more in later
/// versions.
#[derive(Debug)]
#[non_exhaustive]
pub struct FontLoadFailure {
    /// The font file that failed, or that holds the face that failed; `None`
    /// when the font was loaded from memory.
    #[cfg(feature = "fs")]
    pub path: Option<std::path::PathBuf>,

    /// The index of the face that failed within its font; `None` when the
    /// file could not be read, so none of its faces was tried.
    pub face_index: Option<u32>,

    /// Why it did not load.
    pub reason: FontLoadFailureReason,
}

impl FontLoadFailure {
    /// The failure of the face at `index` of the font at `source`.
    fn of_face(source: &Source, index: u32, reason: FontLoadFailureReason) -> Self {
        match source {
            Source::Binary(_) => Self {
                #[cfg(feature = "fs")]
                path: None,
                face_index: Some(index),
                reason,
            },
            #[cfg(feature = "fs")]
            Source::File(path) => Self {
                path: Some(path.to_path_buf()),
                face_index: Some(index),
                reason,
            },
        }
    }

    /// The failure of the font file at `path`, which cannot be read.
    #[cfg(feature = "fs")]
    fn of_file(path: &std::path::Path, error: std::io::Error) -> Self {
        Self {
            path: Some(path.to_path_buf()),
            face_index: None,
            reason: FontLoadFailureReason::UnreadableFile(error),
        }
    }
}

impl core::fmt::Display for FontLoadFailure {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.face_index {
            Some(index) => write!(f, "cannot load face {index}")?,
            None => f.write_str("cannot load font file")?,
        }
        #[cfg(feature = "fs")]
        if let Some(path) = &self.path {
            let of = if self.face_index.is_some() { " of" } else { "" };
            write!(f, "{of} {}", path.display())?;
        }
        write!(f, ": {}", self.reason)
    }
}

/// Why a font file or one of its faces did not load.
///
/// The enum is non-exhaustive, so callers should include a fallback match arm.
#[derive(Debug)]
#[non_exhaustive]
pub enum FontLoadFailureReason {
    /// The file cannot be opened or read, or, with the `memmap` feature,
    /// cannot be memory-mapped. `Display` writes the I/O error.
    #[cfg(feature = "fs")]
    UnreadableFile(std::io::Error),

    /// The data is not a TrueType or OpenType font, or the part of it that
    /// lists the face's tables is damaged.
    MalformedFont,

    /// The face has no family name the database can read.
    NoFamilyName,
}

impl core::fmt::Display for FontLoadFailureReason {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            #[cfg(feature = "fs")]
            Self::UnreadableFile(error) => write!(f, "{error}"),
            Self::MalformedFont => f.write_str("malformed font"),
            Self::NoFamilyName => f.write_str("font doesn't have a family name"),
        }
    }
}

/// Stores font faces and finds the face that best matches a [`Query`].
///
/// Faces are added by the `load_*` methods or [`push_face_info`], and read
/// back with [`face`], [`faces`], [`family_faces`] and [`query`]. The
/// database also holds the family each CSS generic family
/// ([`GenericFamily`]) resolves to.
///
/// # Load order
///
/// A database only gains faces; none is ever removed. It keeps them in load
/// order, the order they were added in: one load after another, and within a
/// load the order that load documents. A collection adds its faces in the
/// order the file holds them. [`faces`] and [`family_faces`] return faces in
/// load order, an [`ID`] is a face's position in it, and when a [`query`]
/// finds several faces equally good, the one loaded first wins. To drop
/// faces, build a new database.
///
/// # Font files
///
/// The file-system loads ([`load_fonts_dir`], [`load_system_fonts`] and
/// [`load_font_file`]) load each font file once per database. The database
/// remembers each font file they reach, by canonical path, and whether it gave
/// a face:
///
/// - A file that gave a face is skipped by every later load.
/// - A file that gave none, because it could not be read or none of its faces
///   parsed, is skipped by the directory and system loads, so loading a
///   directory again does not read its broken files again. [`load_font_file`]
///   reads it again, since its caller asked for that file and should learn why
///   it gives no face.
///
/// Copies of a font and hard links to it are different files and each load. A
/// clone remembers what the original loaded; to load files again, use a new
/// database.
///
/// # Failures
///
/// Every load returns a [`FontLoadReport`]: the faces it added, how many font
/// files it skipped as already loaded, and each file or face it could not
/// load, with the reason. A load neither logs these failures nor stops at
/// one; the report is the only place they appear.
///
/// [`load_fonts_dir`]: Database::load_fonts_dir
/// [`load_system_fonts`]: Database::load_system_fonts
/// [`load_font_file`]: Database::load_font_file
/// [`push_face_info`]: Database::push_face_info
/// [`face`]: Database::face
/// [`faces`]: Database::faces
/// [`family_faces`]: Database::family_faces
/// [`query`]: Database::query
#[derive(Clone, Debug)]
pub struct Database {
    /// Every face, in load order; a face's [`ID`] is its index here.
    faces: Vec<FaceInfo>,
    generic_families: GenericFamilies<String>,
    /// The canonical path of every font file a file-system load reached,
    /// with whether the file gave a face. [`Database::load_font_file`]
    /// leaves out a file it could not read.
    #[cfg(feature = "fs")]
    loaded_files: std::collections::HashMap<std::path::PathBuf, bool>,
}

impl Default for Database {
    fn default() -> Self {
        Self::new()
    }
}

impl Database {
    /// Creates an empty database.
    ///
    /// The generic font families start as:
    ///
    /// - `serif` - Times New Roman
    /// - `sans-serif` - Arial
    /// - `cursive` - Comic Sans MS
    /// - `fantasy` - Impact (Papyrus on macOS/iOS)
    /// - `monospace` - Courier New
    #[inline]
    pub fn new() -> Self {
        Database {
            faces: Vec::new(),
            generic_families: GenericFamilies::defaults(cfg!(any(
                target_os = "macos",
                target_os = "ios"
            ))),
            #[cfg(feature = "fs")]
            loaded_files: std::collections::HashMap::new(),
        }
    }

    /// Loads every face in the font held in `data`.
    ///
    /// A collection adds one face per font it contains. A face that fails to
    /// parse is skipped and listed in the report's failures, so data that is
    /// not a font adds nothing and reports one failure.
    pub fn load_font_data(&mut self, data: Vec<u8>) -> FontLoadReport {
        self.load_font_source(&Source::Binary(alloc::sync::Arc::new(data)))
    }

    /// Loads every face in the font at `source`.
    ///
    /// A collection adds one face per font it contains. A face that fails to
    /// parse is skipped and listed in the report's failures, as is a
    /// [`Source::File`] that cannot be read. A file loaded this way is not
    /// remembered, so it loads again on every call.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use fontgres_core::{Database, Source};
    ///
    /// # let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fonts/Tuffy.ttf");
    /// let mut db = Database::new();
    /// let report = db.load_font_source(&Source::Binary(Arc::new(std::fs::read(path)?)));
    /// let ids: Vec<_> = report.added().collect();
    /// assert_eq!(ids.len(), 1);
    /// assert_eq!(db.face(ids[0]).map(|face| face.index), Some(0));
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn load_font_source(&mut self, source: &Source) -> FontLoadReport {
        let start = self.faces.len();
        let mut failures = Vec::new();
        match source {
            Source::Binary(data) => {
                let faces = parse_faces(source, data.as_ref().as_ref());
                self.push_parsed_faces(faces, &mut failures);
            }
            #[cfg(feature = "fs")]
            Source::File(path) => match with_file_data(path, |data| parse_faces(source, data)) {
                Ok(faces) => self.push_parsed_faces(faces, &mut failures),
                Err(error) => failures.push(FontLoadFailure::of_file(path, error)),
            },
        }
        self.report_since(start, 0, failures)
    }

    /// The report of a load that began when the database held `start` faces.
    fn report_since(
        &self,
        start: usize,
        skipped: usize,
        failures: Vec<FontLoadFailure>,
    ) -> FontLoadReport {
        FontLoadReport {
            added: start..self.faces.len(),
            skipped,
            failures,
        }
    }

    /// Adds the faces of one font that parsed, in order, and moves the
    /// failures of the others to `failures`.
    fn push_parsed_faces(
        &mut self,
        faces: Vec<Result<FaceInfo, FontLoadFailure>>,
        failures: &mut Vec<FontLoadFailure>,
    ) {
        for face in faces {
            match face {
                Ok(info) => {
                    self.push_face_info(info);
                }
                Err(failure) => failures.push(failure),
            }
        }
    }

    /// Returns whether a load skips the font file at `canonical` as already
    /// loaded.
    ///
    /// A file that gave a face is always skipped. A file that was reached and
    /// gave none is skipped too, unless the caller `named` it: a walk passes
    /// over the broken files it has met before, and a load of one file by its
    /// path reads the file again to report why it gives no face.
    #[cfg(feature = "fs")]
    fn skips_font_file(&self, canonical: &std::path::Path, named: bool) -> bool {
        self.loaded_files
            .get(canonical)
            .is_some_and(|&gave_a_face| gave_a_face || !named)
    }

    /// Loads every face in the font file at `path`.
    ///
    /// A collection adds one face per font it contains. A face that fails to
    /// parse is skipped and listed in the report's failures, so a readable
    /// file that is not a font returns `Ok` with a failure and no face added.
    ///
    /// A file that already gave this database a face, by this or any other
    /// file-system load, is skipped: the report counts it and adds nothing. A
    /// file that an earlier load reached and got no face from is read again.
    ///
    /// # Errors
    ///
    /// Returns the I/O error when the file cannot be opened or read, or, with
    /// the `memmap` feature, cannot be memory-mapped. The database is
    /// unchanged.
    #[cfg(feature = "fs")]
    pub fn load_font_file<P: AsRef<std::path::Path>>(
        &mut self,
        path: P,
    ) -> Result<FontLoadReport, std::io::Error> {
        self.load_font_file_impl(path.as_ref())
    }

    // A non-generic version.
    #[cfg(feature = "fs")]
    fn load_font_file_impl(
        &mut self,
        path: &std::path::Path,
    ) -> Result<FontLoadReport, std::io::Error> {
        let start = self.faces.len();
        let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        if self.skips_font_file(&canonical, true) {
            return Ok(self.report_since(start, 1, Vec::new()));
        }
        let faces = parse_font_file(path)?;
        let mut failures = Vec::new();
        self.push_parsed_faces(faces, &mut failures);
        self.loaded_files
            .insert(canonical, self.faces.len() > start);
        Ok(self.report_since(start, 0, failures))
    }

    /// Adds the faces of `files` in the order of `files`, as loading them one
    /// by one would, after parsing them on several threads. A file this
    /// database already reached, in an earlier load or earlier in `files`, is
    /// skipped. A file that cannot be read is a failure in the report.
    #[cfg(feature = "fs")]
    fn load_font_files(&mut self, files: Vec<FontFile>) -> FontLoadReport {
        let start = self.faces.len();
        let reached = files.len();
        let mut new = Vec::with_capacity(reached);
        for file in files {
            if !self.skips_font_file(&file.canonical, false) {
                // Remembered before it is parsed, so a second path to the
                // file later in `files` is skipped.
                self.loaded_files.insert(file.canonical.clone(), false);
                new.push(file);
            }
        }
        let skipped = reached.saturating_sub(new.len());

        let mut failures = Vec::new();
        let parsed_files = parse_font_files(&new);
        for (file, parsed) in new.into_iter().zip(parsed_files) {
            let faces_before = self.faces.len();
            match parsed {
                Ok(faces) => self.push_parsed_faces(faces, &mut failures),
                Err(error) => failures.push(FontLoadFailure::of_file(&file.path, error)),
            }
            if self.faces.len() > faces_before {
                self.loaded_files.insert(file.canonical, true);
            }
        }
        self.report_since(start, skipped, failures)
    }

    /// Loads every font file in `dir` and its subdirectories.
    ///
    /// Only files with a `ttf`, `otf`, `ttc` or `otc` extension, in any case,
    /// load. Symbolic links are followed, and a link loads when either its own
    /// name or its target's has such an extension. A file loads once however
    /// many links lead to it, and a file this database already reached, by
    /// this or any other file-system load, is skipped, whether or not it gave
    /// a face. The report counts every path to a file that was skipped.
    /// Loading a directory again adds only the files added to it since.
    ///
    /// The entries of each directory load in the order of their file names,
    /// compared as bytes, so `B.ttf` loads before `a.ttf`. Files, subdirectories
    /// and links sort together, each by its own name, and a subdirectory's
    /// fonts load where its name falls. The order does not depend on the file
    /// system, so the same fonts load in the same order on every machine.
    ///
    /// A file that cannot be read, and a face that fails to parse, is skipped
    /// and listed in the report's failures. A subdirectory that cannot be read
    /// adds nothing and is not reported.
    ///
    /// # Errors
    ///
    /// Returns the I/O error when `dir` itself cannot be read: it does not
    /// exist, is not a directory, or may not be listed. The database is
    /// unchanged.
    #[cfg(feature = "fs")]
    pub fn load_fonts_dir<P: AsRef<std::path::Path>>(
        &mut self,
        dir: P,
    ) -> Result<FontLoadReport, std::io::Error> {
        let mut walk = FontDirWalk::default();
        walk.find_font_files_in_readable(dir.as_ref())?;
        Ok(self.load_font_files(walk.files))
    }

    /// Loads the fonts installed on the system by scanning its font directories.
    ///
    /// The directories scanned are fixed per platform rather than asked of the
    /// system's font APIs, so fonts installed elsewhere must be loaded with
    /// [`load_fonts_dir`](Database::load_fonts_dir) or
    /// [`load_font_file`](Database::load_font_file):
    ///
    /// - Windows: `%SYSTEMROOT%\Fonts`, or `C:\Windows\Fonts` when the
    ///   variable is unset or not Unicode, and the per-user font directories
    ///   under `%USERPROFILE%\AppData`.
    /// - macOS and iOS: `/Library/Fonts`, `/System/Library/Fonts`, the
    ///   downloadable fonts under `/System/Library/AssetsV2`,
    ///   `/Network/Library/Fonts` and `~/Library/Fonts`.
    /// - Other Unix systems except Android, Redox included: with the `fontconfig` feature, the
    ///   directories fontconfig's configuration lists. Without the feature, or
    ///   when the configuration lists no directory, `/usr/share/fonts`,
    ///   `/usr/local/share/fonts`, `~/.fonts` and `~/.local/share/fonts`.
    ///   With the feature, once the fonts are loaded, each generic family that
    ///   fontconfig has an `<alias>` for becomes the first installed family in
    ///   the order fontconfig tries them (every alias's `prefer` list, then
    ///   `accept`, then `default`), or the first one tried when none is
    ///   installed.
    ///
    /// Other platforms load nothing. The directories load in the order listed
    /// here, or in the order fontconfig's configuration lists them, and each
    /// loads in the order [`load_fonts_dir`](Database::load_fonts_dir)
    /// documents; the downloadable font directories load in the order of their
    /// names.
    ///
    /// Nothing here is an error: a directory that does not exist or cannot be
    /// read adds nothing and is not reported. The report lists the files and
    /// faces that failed and counts the files skipped, as the one
    /// [`load_fonts_dir`](Database::load_fonts_dir) returns does.
    #[cfg(feature = "fs")]
    pub fn load_system_fonts(&mut self) -> FontLoadReport {
        let found = system_fonts::find(&env::SystemEnv, std::path::Path::new("/"));
        self.load_found_system_fonts(&found)
    }

    /// Loads the font directories `found` lists, in its order, then points
    /// each generic family its aliases name at a family.
    #[cfg(feature = "fs")]
    fn load_found_system_fonts(&mut self, found: &system_fonts::SystemFonts) -> FontLoadReport {
        let mut walk = FontDirWalk::default();
        for dir in &found.dirs {
            walk.find_font_files(dir);
        }
        let report = self.load_font_files(walk.files);
        // Generic families resolve after loading, so each can skip the
        // families fontconfig prefers that are not installed.
        self.set_generic_families(&found.aliases);
        report
    }

    /// Points each generic family that `aliases` name at the first installed
    /// family in the order fontconfig tries them, or at the first one tried
    /// when none is installed.
    #[cfg(feature = "fs")]
    fn set_generic_families(&mut self, aliases: &[generic_family::Alias]) {
        for generic in GenericFamily::ALL {
            let resolved = generic_family::resolve(aliases, generic, |name| {
                self.family_faces(name).next().is_some()
            })
            .map(str::to_owned);
            if let Some(family) = resolved {
                self.set_generic_family(generic, family);
            }
        }
    }

    /// Adds a face described by the caller and returns its new ID.
    ///
    /// Use it to replace the metadata a font declares with your own: the face
    /// data at `info.source` is not parsed or even read. Whatever
    /// [`FaceInfo::id`] holds is replaced by the new ID, so fill it with
    /// [`ID::dummy`].
    ///
    /// The face goes last in the database's load order.
    pub fn push_face_info(&mut self, mut info: FaceInfo) -> ID {
        let id = ID(self.faces.len());
        info.id = id;
        self.faces.push(info);
        id
    }

    /// Returns `true` if the `Database` contains no font faces.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }

    /// Returns the number of font faces in the `Database`.
    ///
    /// The database stores faces, not fonts, so a collection (`*.ttc`) that
    /// holds five faces counts five.
    #[inline]
    pub fn len(&self) -> usize {
        self.faces.len()
    }

    /// Sets the family that `generic` resolves to.
    ///
    /// # Examples
    ///
    /// ```
    /// use fontgres_core::{Database, Family, GenericFamily};
    ///
    /// let mut db = Database::new();
    /// db.set_generic_family(GenericFamily::Monospace, "Iosevka");
    /// let monospace = Family::Generic(GenericFamily::Monospace);
    /// assert_eq!(db.family_name(&monospace), "Iosevka");
    /// ```
    pub fn set_generic_family<S: Into<String>>(&mut self, generic: GenericFamily, family: S) {
        *self.generic_families.get_mut(generic) = family.into();
    }

    /// Returns the family name `family` resolves to in this database.
    ///
    /// A [`Family::Name`] resolves to its own name. A generic family resolves to
    /// the family [`set_generic_family`](Database::set_generic_family) gave
    /// it, or to the default listed on [`Database::new`].
    pub fn family_name<'a>(&'a self, family: &'a Family<'_>) -> &'a str {
        match family {
            Family::Name(name) => name,
            Family::Generic(generic) => self.generic_families.get(*generic),
        }
    }

    /// Returns the face that best matches `query`, or `None` when no face has
    /// any of its families.
    ///
    /// The families are tried in order, each resolved through
    /// [`family_name`](Database::family_name). The first family that some face
    /// carries, under any of its names in any language, decides the result:
    /// among that family's faces the best stretch, then style, then weight is
    /// chosen as the CSS font matching algorithm does. Family names compare
    /// ignoring ASCII case, as [`FaceInfo::has_family`] does.
    ///
    /// When several faces of the family are equal in stretch, style and
    /// weight, the one earliest in load order is returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use fontgres_core::{Database, Family, Query};
    ///
    /// # let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fonts/Tuffy.ttf");
    /// let mut db = Database::new();
    /// assert!(db.load_font_data(std::fs::read(path)?).failures().is_empty());
    ///
    /// let families = [Family::Name("Missing"), Family::Name("Tuffy")];
    /// assert!(db.query(&Query { families: &families, ..Query::default() }).is_some());
    ///
    /// let families = [Family::Name("tuffy")];
    /// assert!(db.query(&Query { families: &families, ..Query::default() }).is_some());
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn query(&self, query: &Query<'_>) -> Option<&FaceInfo> {
        query.families.iter().find_map(|family| {
            let candidates: Vec<_> = self.family_faces(self.family_name(family)).collect();
            find_best_match(&candidates, query)
        })
    }

    /// Returns every face in the database, in load order.
    ///
    /// The face with [`ID`] `id` is the one [`face`](Database::face) returns
    /// for it. Use the slice for matching that [`query`](Database::query) does
    /// not cover.
    #[inline]
    pub fn faces(&self) -> &[FaceInfo] {
        &self.faces
    }

    /// Returns the faces of the family `name`, in load order.
    ///
    /// A face belongs to the family when `name` is one of its family names in
    /// any language, ignoring ASCII case, as [`FaceInfo::has_family`] tells.
    /// A generic family is a [`Family`], not a name; resolve it with
    /// [`family_name`](Database::family_name) first.
    ///
    /// # Examples
    ///
    /// ```
    /// use fontgres_core::Database;
    ///
    /// # let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fonts/Tuffy.ttf");
    /// let mut db = Database::new();
    /// assert!(db.load_font_data(std::fs::read(path)?).failures().is_empty());
    ///
    /// assert_eq!(db.family_faces("tuffy").count(), 1);
    /// assert_eq!(db.family_faces("Tuffy Sans").count(), 0);
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn family_faces<'a>(&'a self, name: &str) -> impl Iterator<Item = &'a FaceInfo> {
        self.faces.iter().filter(move |face| face.has_family(name))
    }

    /// Returns the face with `id`.
    ///
    /// Returns `None` for [`ID::dummy`] and for an ID that came from another
    /// database. See [`ID`] for why an ID from another database can also
    /// return an unrelated face.
    pub fn face(&self, id: ID) -> Option<&FaceInfo> {
        self.faces.get(id.0)
    }

    /// Calls `p` with the data of the font holding the face with `id`, and
    /// the face's index in that font, and returns what `p` returns.
    ///
    /// The data is lent to the closure rather than returned because a file is
    /// only read, or memory-mapped with the `memmap` feature, for the length of
    /// the call. Pass the two arguments to a font parser to read what
    /// [`FaceInfo`] does not hold.
    ///
    /// Returns `None` without calling `p` when no face has `id`, or when the
    /// face's file cannot be read.
    ///
    /// # Examples
    ///
    /// ```
    /// use fontgres_core::Database;
    ///
    /// # let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fonts/Tuffy.ttf");
    /// let mut db = Database::new();
    /// assert!(db.load_font_data(std::fs::read(path)?).failures().is_empty());
    /// let id = db.faces().first().expect("Tuffy is loaded").id;
    ///
    /// let (magic, index) = db
    ///     .with_face_data(id, |data, index| (data[..4].to_vec(), index))
    ///     .expect("in-memory data is always readable");
    /// assert_eq!(magic, [0x00, 0x01, 0x00, 0x00]); // a TrueType font
    /// assert_eq!(index, 0);
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn with_face_data<P, T>(&self, id: ID, p: P) -> Option<T>
    where
        P: FnOnce(&[u8], u32) -> T,
    {
        let face = self.face(id)?;
        face.source.with_data(|data| p(data, face.index))
    }
}

/// Describes one face in a [`Database`]: where its data is, and the
/// properties a [`Query`] matches on.
///
/// A font can hold several faces; each is a separate `FaceInfo`.
#[derive(Clone, Debug)]
pub struct FaceInfo {
    /// The face's ID in its database: its position in the load order.
    pub id: ID,

    /// Where the font holding the face is stored.
    ///
    /// The faces of a collection share one source.
    pub source: Source,

    /// The face's index within the font at [`source`](FaceInfo::source); zero
    /// unless the font is a collection.
    pub index: u32,

    /// The face's family names, each with its language.
    ///
    /// The first is the English (United States) name when the font has one.
    /// A face loaded from font data always has at least one: a face without a
    /// family name does not load.
    ///
    /// Corresponds to a *Typographic Family* (ID 16) or a *Font Family* (ID 1) [name ID]
    /// in a TrueType font.
    ///
    /// This is not an *Extended Typographic Family* or a *Full Name*.
    /// Meaning it will contain _Arial_ and not _Arial Bold_.
    ///
    /// [name ID]: https://docs.microsoft.com/en-us/typography/opentype/spec/name#name-ids
    pub families: Vec<(String, Language)>,

    /// The face's PostScript name.
    ///
    /// Corresponds to a *PostScript name* (6) [name ID] in a TrueType font.
    /// A font without one gets a name made from its first family, as
    /// fontconfig makes it: spaces and the other characters PostScript forbids
    /// in a name become `-`, so *DEC Terminal Modern* gives
    /// `DEC-Terminal-Modern`.
    ///
    /// [name ID]: https://docs.microsoft.com/en-us/typography/opentype/spec/name#name-ids
    pub post_script_name: String,

    /// The face's style.
    ///
    /// A face whose `OS/2` table declares it normal but whose `post` table
    /// gives an italic angle is [`Style::Italic`].
    pub style: Style,

    /// The face's weight, as its `OS/2` table declares it, or
    /// [`Weight::NORMAL`] when it has none.
    pub weight: Weight,

    /// The face's stretch, as its `OS/2` table declares it, or
    /// [`Stretch::Normal`] when it has none.
    pub stretch: Stretch,

    /// Whether the face's `post` table declares it monospaced; `false` when it
    /// has none.
    pub monospaced: bool,
}

impl FaceInfo {
    /// Returns whether `name` is one of the face's
    /// [`families`](FaceInfo::families), ignoring ASCII case.
    ///
    /// Letters outside ASCII must match in case as well, although CSS compares
    /// family names with Unicode caseless matching.
    ///
    /// # Examples
    ///
    /// ```
    /// use fontgres_core::Database;
    ///
    /// # let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fonts/Tuffy.ttf");
    /// let mut db = Database::new();
    /// assert!(db.load_font_data(std::fs::read(path)?).failures().is_empty());
    /// let face = db.faces().first().expect("Tuffy is loaded");
    /// assert!(face.has_family("TUFFY"));
    /// assert!(!face.has_family("Tuffy Sans"));
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn has_family(&self, name: &str) -> bool {
        self.families
            .iter()
            .any(|(family, _)| family.eq_ignore_ascii_case(name))
    }
}

/// Holds or locates the data of a whole font, which may contain several faces.
#[derive(Clone)]
pub enum Source {
    /// Font data in memory, typically a `Vec<u8>`.
    Binary(alloc::sync::Arc<dyn AsRef<[u8]> + Sync + Send>),

    /// A font file, read each time its data is needed.
    ///
    /// The faces of a collection share one path, so cloning the source copies
    /// no path.
    #[cfg(feature = "fs")]
    File(std::sync::Arc<std::path::Path>),
}

impl core::fmt::Debug for Source {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Binary(arg0) => f
                .debug_tuple("SharedBinary")
                .field(&arg0.as_ref().as_ref())
                .finish(),
            #[cfg(feature = "fs")]
            Self::File(arg0) => f.debug_tuple("File").field(arg0).finish(),
        }
    }
}

/// Whether `path` ends in a font extension, `ttf`, `ttc`, `otf` or `otc`, in
/// any case.
#[cfg(feature = "fs")]
fn has_font_extension(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            ["ttf", "ttc", "otf", "otc"]
                .iter()
                .any(|font| extension.eq_ignore_ascii_case(font))
        })
}

/// A font file a walk found: the path to load and report it by, and the
/// canonical path that identifies it however many paths lead to it.
#[cfg(feature = "fs")]
struct FontFile {
    path: std::path::PathBuf,
    canonical: std::path::PathBuf,
}

/// A walk over font directories: the canonical paths of the directories
/// reached, to walk each once however many paths lead to it, and the font
/// files found, in walk order. A walk takes the entries of a directory in the
/// order of their file names. A file reached by several paths is listed once
/// per path; loading skips all but the first.
#[cfg(feature = "fs")]
#[derive(Default)]
struct FontDirWalk {
    dirs: std::collections::HashSet<std::path::PathBuf>,
    files: Vec<FontFile>,
}

#[cfg(feature = "fs")]
impl FontDirWalk {
    /// Adds the font files in `dir` and its subdirectories, in the order
    /// [`load_fonts_dir`](Database::load_fonts_dir) documents, skipping each
    /// directory the walk already reached by another path. A `dir` that
    /// cannot be read adds nothing.
    fn find_font_files(&mut self, dir: &std::path::Path) {
        if let Some(canonical) = self.enter_root(dir) {
            self.walk_dir(dir, &canonical);
        }
    }

    /// Does what [`find_font_files`](Self::find_font_files) does, for a
    /// `dir` the caller expects to read: returns the error that keeps it from
    /// being read.
    fn find_font_files_in_readable(&mut self, dir: &std::path::Path) -> std::io::Result<()> {
        if let Some(canonical) = self.enter_root(dir) {
            self.walk_entries(std::fs::read_dir(dir)?, &canonical);
        }
        Ok(())
    }

    /// Returns the canonical path of the root `dir`, or `None` when the walk
    /// already reached that directory.
    fn enter_root(&mut self, dir: &std::path::Path) -> Option<std::path::PathBuf> {
        // Canonicalizing the root once and joining each entry's name to it
        // gives every entry that is not a link its canonical path, so the walk
        // costs one canonicalization per root and per link, not per file. A
        // root that can be read but not canonicalized is still walked, under
        // the path it was given.
        let canonical = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
        self.dirs.insert(canonical.clone()).then_some(canonical)
    }

    /// Walks `dir`, whose canonical path is `canonical`. A directory that
    /// cannot be read adds nothing.
    fn walk_dir(&mut self, dir: &std::path::Path, canonical: &std::path::Path) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            self.walk_entries(entries, canonical);
        }
    }

    /// Walks the `entries` of a directory whose canonical path is
    /// `canonical`. Files keep the path they were found at.
    fn walk_entries(&mut self, entries: std::fs::ReadDir, canonical: &std::path::Path) {
        // The file system's order is not the same on every machine, and load
        // order decides which of two equally good faces a query returns.
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_cached_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if file_type.is_symlink() {
                self.follow_link(&path);
            } else if file_type.is_dir() {
                let key = canonical.join(entry.file_name());
                if self.dirs.insert(key.clone()) {
                    self.walk_dir(&path, &key);
                }
            } else if file_type.is_file() && has_font_extension(&path) {
                self.files.push(FontFile {
                    canonical: canonical.join(entry.file_name()),
                    path,
                });
            }
        }
    }

    /// Adds the font file or walks the directory `link` leads to, by its
    /// canonical path.
    fn follow_link(&mut self, link: &std::path::Path) {
        let Ok(stat) = std::fs::metadata(link) else {
            return;
        };
        if !stat.is_file() && !stat.is_dir() {
            return;
        }
        let Ok(target) = std::fs::canonicalize(link) else {
            return;
        };
        // A link counts when either its own name or its target's has a font
        // extension: content-addressed stores link font names to files named
        // by hash.
        if stat.is_file() && !has_font_extension(link) && !has_font_extension(&target) {
            return;
        }
        if stat.is_dir() {
            if self.dirs.insert(target.clone()) {
                self.walk_dir(&target, &target);
            }
        } else {
            self.files.push(FontFile {
                path: target.clone(),
                canonical: target,
            });
        }
    }
}

/// Every face of a font, in index order: the face, or why it did not parse.
type ParsedFaces = Vec<Result<FaceInfo, FontLoadFailure>>;

/// Parses every face of the font in `data`, which `source` holds or locates.
fn parse_faces(source: &Source, data: &[u8]) -> ParsedFaces {
    let n = ttf_parser::fonts_in_collection(data).unwrap_or(1);
    (0..n)
        .map(|index| {
            parse_face_info(source.clone(), data, index)
                .map_err(|reason| FontLoadFailure::of_face(source, index, reason))
        })
        .collect()
}

/// Every face of a font file, or the error that kept the file from being read.
#[cfg(feature = "fs")]
type ParsedFontFile = Result<ParsedFaces, std::io::Error>;

/// Parses every face of the font file at `path`.
#[cfg(feature = "fs")]
fn parse_font_file(path: &std::path::Path) -> ParsedFontFile {
    with_file_data(path, |data| parse_faces(&Source::File(path.into()), data))
}

/// The most threads [`parse_font_files`] uses. Opening and mapping files
/// contend in the kernel: on a 16-core Mac, more than six threads made loading
/// slower while multiplying the CPU time spent.
#[cfg(feature = "fs")]
const MAX_LOAD_THREADS: usize = 4;

/// The fewest files [`parse_font_files`] gives each thread, so that a few
/// files are not worth a thread's start.
#[cfg(feature = "fs")]
const MIN_FILES_PER_LOAD_THREAD: usize = 16;

/// Parses `files` with [`parse_font_file`] on up to [`MAX_LOAD_THREADS`]
/// threads and returns the results in the order of `files`.
///
/// Opening and mapping a file costs more than parsing it, and the threads
/// overlap that wait.
#[cfg(feature = "fs")]
fn parse_font_files(files: &[FontFile]) -> Vec<ParsedFontFile> {
    use core::sync::atomic::{AtomicUsize, Ordering};

    let threads = std::thread::available_parallelism()
        .map_or(1, core::num::NonZero::get)
        .min(MAX_LOAD_THREADS)
        .min(files.len().div_ceil(MIN_FILES_PER_LOAD_THREAD));
    let next = AtomicUsize::new(0);
    // Each thread takes the next unparsed file until none is left.
    let work = || {
        let mut parsed = Vec::new();
        loop {
            let index = next.fetch_add(1, Ordering::Relaxed);
            let Some(file) = files.get(index) else {
                break parsed;
            };
            parsed.push((index, parse_font_file(&file.path)));
        }
    };
    let mut parsed = std::thread::scope(|scope| {
        // A thread that fails to spawn leaves its share to the others; this
        // thread works too.
        let workers: Vec<_> = (1..threads)
            .filter_map(|_| {
                std::thread::Builder::new()
                    .name("fontgres-load".into())
                    .spawn_scoped(scope, work)
                    .ok()
            })
            .collect();
        let mut parsed = work();
        for worker in workers {
            match worker.join() {
                Ok(more) => parsed.extend(more),
                Err(panic) => std::panic::resume_unwind(panic),
            }
        }
        parsed
    });
    parsed.sort_unstable_by_key(|&(index, _)| index);
    parsed.into_iter().map(|(_, file)| file).collect()
}

/// Calls `f` with the contents of the file at `path` and returns what `f`
/// returns.
///
/// With the `memmap` feature the file is memory-mapped for the length of the
/// call; without it, the file is read into memory.
#[cfg(feature = "fs")]
fn with_file_data<T, F>(path: &std::path::Path, f: F) -> Result<T, std::io::Error>
where
    F: FnOnce(&[u8]) -> T,
{
    #[cfg(not(feature = "memmap"))]
    let data = std::fs::read(path)?;
    #[cfg(feature = "memmap")]
    let data = {
        let file = std::fs::File::open(path)?;
        #[expect(
            unsafe_code,
            reason = "mapping reads only the pages `f` touches; nothing enforces that the file stays unchanged, as the crate docs disclose"
        )]
        // SAFETY:
        // Operation: `MmapOptions::map(&file)`.
        // Contract (memmap2 0.9, `Mmap` docs): using the map is undefined
        // behavior if the underlying file is modified, in or out of process,
        // while the map is in use.
        // Not discharged: nothing here stops another process, another thread,
        // a `log` backend, or `f` from truncating or rewriting the file while
        // the map is alive. Caller code runs while it is: the hint below can
        // log, and `Database::with_face_data` passes the caller's closure as
        // `f`. The loads pass a parse, which runs no caller code. The crate
        // docs disclose this under "Memory-mapped files". The bound `F: FnOnce(&[u8]) -> T` keeps `f` from holding on
        // to the borrowed bytes, so the map drops when this function returns,
        // which ends the exposure.
        let mmap = unsafe { memmap2::MmapOptions::new().map(&file)? };
        // Parsing a face reads a few KiB of tables (`name`, `OS/2`, `post`)
        // spread over the file, and a font parser handed the data jumps between
        // tables too. With the default sequential readahead each page fault
        // pulls a whole readahead window, up to 4 MiB on btrfs, so listing
        // thousands of installed faces reads gigabytes. The hint only affects
        // performance, so a failure is logged and ignored.
        #[cfg(unix)]
        if let Err(error) = mmap.advise(memmap2::Advice::Random) {
            log::debug!(
                "Failed to mark '{}' as random access: {}.",
                path.display(),
                error
            );
        }
        mmap
    };
    Ok(f(&data))
}

impl Source {
    #[cfg_attr(
        not(feature = "fs"),
        expect(
            clippy::unnecessary_wraps,
            reason = "without the fs feature every source is in memory and always has data; with it, a file can fail to read"
        )
    )]
    fn with_data<P, T>(&self, p: P) -> Option<T>
    where
        P: FnOnce(&[u8]) -> T,
    {
        match &self {
            #[cfg(feature = "fs")]
            Source::File(path) => with_file_data(path, p).ok(),
            Source::Binary(data) => Some(p(data.as_ref().as_ref())),
        }
    }
}

/// Describes the face to find with [`Database::query`], as CSS font
/// properties.
///
/// The default query asks for no family at normal weight, style and stretch,
/// so set [`families`](Query::families) and fill the rest with
/// `..Query::default()`.
#[derive(Clone, Copy, Default, Debug, Eq, PartialEq, Hash)]
pub struct Query<'a> {
    /// A prioritized list of font family names or generic family names.
    ///
    /// [font-family](https://www.w3.org/TR/2018/REC-css-fonts-3-20180920/#propdef-font-family) in CSS.
    pub families: &'a [Family<'a>],

    /// Specifies the weight of glyphs in the font, their degree of blackness or stroke thickness.
    ///
    /// [font-weight](https://www.w3.org/TR/2018/REC-css-fonts-3-20180920/#font-weight-prop) in CSS.
    pub weight: Weight,

    /// Selects a normal, condensed, or expanded face from a font family.
    ///
    /// [font-stretch](https://www.w3.org/TR/2018/REC-css-fonts-3-20180920/#font-stretch-prop) in CSS.
    pub stretch: Stretch,

    /// Allows italic or oblique faces to be selected.
    ///
    /// [font-style](https://www.w3.org/TR/2018/REC-css-fonts-3-20180920/#font-style-prop) in CSS.
    pub style: Style,
}

/// Names a [font family](https://www.w3.org/TR/2018/REC-css-fonts-3-20180920/#propdef-font-family)
/// or a CSS generic family.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Family<'a> {
    /// The name of a font family of choice.
    ///
    /// This must be a *Typographic Family* (ID 16) or a *Family Name* (ID 1) in terms of TrueType.
    /// Meaning you have to pass a family without any additional suffixes like _Bold_, _Italic_,
    /// _Regular_, etc.
    ///
    /// Localized names are allowed.
    Name(&'a str),

    /// A generic family, which stands for the family the database holds for
    /// it.
    Generic(GenericFamily),
}

/// The weight of a face's glyphs, their degree of blackness or stroke thickness.
///
/// The number is the CSS `font-weight`, from 1 to 1000. The constants name the
/// common weights, from [`THIN`](Weight::THIN) (100) to
/// [`BLACK`](Weight::BLACK) (900); [`NORMAL`](Weight::NORMAL) (400) is the
/// default.
#[derive(Clone, Copy, PartialOrd, Ord, PartialEq, Eq, Debug, Hash)]
pub struct Weight(pub u16);

impl Default for Weight {
    #[inline]
    fn default() -> Weight {
        Weight::NORMAL
    }
}

impl Weight {
    /// Thin weight (100), the thinnest value.
    pub const THIN: Weight = Weight(100);
    /// Extra light weight (200).
    pub const EXTRA_LIGHT: Weight = Weight(200);
    /// Light weight (300).
    pub const LIGHT: Weight = Weight(300);
    /// Normal (400).
    pub const NORMAL: Weight = Weight(400);
    /// Medium weight (500, higher than normal).
    pub const MEDIUM: Weight = Weight(500);
    /// Semibold weight (600).
    pub const SEMIBOLD: Weight = Weight(600);
    /// Bold weight (700).
    pub const BOLD: Weight = Weight(700);
    /// Extra-bold weight (800).
    pub const EXTRA_BOLD: Weight = Weight(800);
    /// Black weight (900), the thickest value.
    pub const BLACK: Weight = Weight(900);
}

/// The slant of a face, as the CSS `font-style`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Style {
    /// A face that is neither italic nor oblique.
    Normal,
    /// A form that is generally cursive in nature.
    Italic,
    /// A typically-sloped version of the regular face.
    Oblique,
}

impl Default for Style {
    #[inline]
    fn default() -> Style {
        Style::Normal
    }
}

fn parse_face_info(
    source: Source,
    data: &[u8],
    index: u32,
) -> Result<FaceInfo, FontLoadFailureReason> {
    let raw_face = ttf_parser::RawFace::parse(data, index)
        .map_err(|_cause| FontLoadFailureReason::MalformedFont)?;
    let (families, post_script_name) =
        parse_names(&raw_face).ok_or(FontLoadFailureReason::NoFamilyName)?;
    let (mut style, weight, stretch) = parse_os2(&raw_face);
    let (monospaced, italic) = parse_post(&raw_face);

    if style == Style::Normal && italic {
        style = Style::Italic;
    }

    Ok(FaceInfo {
        id: ID::dummy(),
        source,
        index,
        families,
        post_script_name,
        style,
        weight,
        stretch,
        monospaced,
    })
}

fn parse_names(raw_face: &ttf_parser::RawFace<'_>) -> Option<(Vec<(String, Language)>, String)> {
    const NAME_TAG: ttf_parser::Tag = ttf_parser::Tag::from_bytes(*b"name");
    let name_data = raw_face.table(NAME_TAG)?;
    let name_table = ttf_parser::name::Table::parse(name_data)?;

    let mut families = collect_families(ttf_parser::name_id::TYPOGRAPHIC_FAMILY, &name_table.names);

    // We have to fallback to Family Name when no Typographic Family Name was set.
    if families.is_empty() {
        families = collect_families(ttf_parser::name_id::FAMILY, &name_table.names);
    }

    // Make English US the first one.
    if let Some(index) = families
        .iter()
        .position(|f| f.1 == Language::English_UnitedStates)
    {
        families.swap(0, index);
    }

    let (first_family, _) = families.first()?;

    // A font without a PostScript name still loads, with one made from its
    // family as fontconfig makes it.
    let post_script_name = name_table
        .names
        .into_iter()
        .find(|name| {
            name.id == ttf_parser::name_id::POST_SCRIPT_NAME && name.is_supported_encoding()
        })
        .and_then(|name| name_to_unicode(&name))
        .unwrap_or_else(|| postscript_name::from_family(first_family));

    Some((families, post_script_name))
}

fn collect_families(name_id: u16, names: &ttf_parser::name::Names<'_>) -> Vec<(String, Language)> {
    let mut families = Vec::new();
    for name in names.into_iter() {
        if name.id == name_id
            && name.is_unicode()
            && let Some(family) = name_to_unicode(&name)
        {
            families.push((family, name.language()));
        }
    }

    // If no Unicode English US family name was found then look for English MacRoman as well.
    if !families
        .iter()
        .any(|f| f.1 == Language::English_UnitedStates)
    {
        for name in names.into_iter() {
            if name.id == name_id
                && name.is_mac_roman()
                && let Some(family) = name_to_unicode(&name)
            {
                families.push((family, name.language()));
                break;
            }
        }
    }

    families
}

fn name_to_unicode(name: &ttf_parser::name::Name<'_>) -> Option<String> {
    if name.is_unicode() {
        let mut raw_data: Vec<u16> = Vec::new();
        for c in ttf_parser::LazyArray16::<u16>::new(name.data) {
            raw_data.push(c);
        }

        String::from_utf16(&raw_data).ok()
    } else if name.is_mac_roman() {
        // We support only MacRoman encoding here, which should be enough in most cases.
        let raw_data: Vec<u16> = name
            .data
            .iter()
            .map(|&byte| mac_roman_to_utf16(byte))
            .collect();

        String::from_utf16(&raw_data).ok()
    } else {
        None
    }
}

fn parse_os2(raw_face: &ttf_parser::RawFace<'_>) -> (Style, Weight, Stretch) {
    const OS2_TAG: ttf_parser::Tag = ttf_parser::Tag::from_bytes(*b"OS/2");
    let Some(table) = raw_face
        .table(OS2_TAG)
        .and_then(ttf_parser::os2::Table::parse)
    else {
        return (Style::Normal, Weight::NORMAL, Stretch::Normal);
    };

    let style = match table.style() {
        ttf_parser::Style::Normal => Style::Normal,
        ttf_parser::Style::Italic => Style::Italic,
        ttf_parser::Style::Oblique => Style::Oblique,
    };

    let weight = table.weight();
    let stretch = table.width();

    (style, Weight(weight.to_number()), stretch)
}

fn parse_post(raw_face: &ttf_parser::RawFace<'_>) -> (bool, bool) {
    // We need just a single value from the `post` table, while ttf-parser will parse all.
    // Therefore we have a custom parser.

    const POST_TAG: ttf_parser::Tag = ttf_parser::Tag::from_bytes(*b"post");
    let Some(data) = raw_face.table(POST_TAG) else {
        return (false, false);
    };

    // All we care about, it that u32 at offset 12 is non-zero.
    let monospaced = data.get(12..16) != Some(&[0, 0, 0, 0]);

    // Italic angle as f16.16.
    let italic = data.get(4..8) != Some(&[0, 0, 0, 0]);

    (monospaced, italic)
}

trait NameExt {
    fn is_mac_roman(&self) -> bool;
    fn is_supported_encoding(&self) -> bool;
}

impl NameExt for ttf_parser::name::Name<'_> {
    #[inline]
    fn is_mac_roman(&self) -> bool {
        use ttf_parser::PlatformId::Macintosh;
        // https://docs.microsoft.com/en-us/typography/opentype/spec/name#macintosh-encoding-ids-script-manager-codes
        const MACINTOSH_ROMAN_ENCODING_ID: u16 = 0;

        self.platform_id == Macintosh && self.encoding_id == MACINTOSH_ROMAN_ENCODING_ID
    }

    #[inline]
    fn is_supported_encoding(&self) -> bool {
        self.is_unicode() || self.is_mac_roman()
    }
}

// Based on https://github.com/servo/font-kit
/// Returns the face CSS font matching picks from one family's `candidates`,
/// or `None` when there are none.
///
/// Implements step 4 of
/// <https://www.w3.org/TR/2018/REC-css-fonts-3-20180920/#font-style-matching>.
/// Step 4d (`font-size`) does not apply. Of the faces left equal after the
/// steps, the first in `candidates` wins; `candidates` come in load order.
#[inline(never)]
fn find_best_match<'a>(candidates: &[&'a FaceInfo], query: &Query<'_>) -> Option<&'a FaceInfo> {
    let mut matching_set = candidates.to_vec();

    // Step 4a (`font-stretch`).
    let stretch = matching_stretch(&matching_set, query.stretch)?;
    matching_set.retain(|face| face.stretch == stretch);

    // Step 4b (`font-style`).
    let style = matching_style(&matching_set, query.style)?;
    matching_set.retain(|face| face.style == style);

    // Step 4c (`font-weight`).
    let weight = matching_weight(&matching_set, query.weight)?;
    matching_set.retain(|face| face.weight == weight);

    matching_set.first().copied()
}

/// Returns the stretch of the face nearest `query`: an exact match, else the
/// nearest narrower one then the nearest wider one for a query up to normal
/// width, and the other way round for a wider query.
fn matching_stretch(faces: &[&FaceInfo], query: Stretch) -> Option<Stretch> {
    nearest(
        faces,
        |face| face.stretch.to_number(),
        query.to_number(),
        query <= Stretch::Normal,
    )
    .map(|face| face.stretch)
}

/// Returns the first style in the fallback order for `query` that one of
/// `faces` has.
fn matching_style(faces: &[&FaceInfo], query: Style) -> Option<Style> {
    let preference = match query {
        Style::Italic => [Style::Italic, Style::Oblique, Style::Normal],
        Style::Oblique => [Style::Oblique, Style::Italic, Style::Normal],
        Style::Normal => [Style::Normal, Style::Oblique, Style::Italic],
    };
    preference
        .into_iter()
        .find(|&style| faces.iter().any(|face| face.style == style))
}

/// Returns the weight of the face nearest `query`: an exact match, else 500
/// for a query from 400 up to 450 and 400 for a query from 450 to 500, else
/// the nearest lighter then the nearest heavier one for a query up to 500,
/// and the other way round for a heavier query.
///
/// The spec does not say which way a query between 400 and 500 looks first,
/// so 450 is the cutoff.
fn matching_weight(faces: &[&FaceInfo], query: Weight) -> Option<Weight> {
    let has = |weight: Weight| faces.iter().any(|face| face.weight == weight);
    if has(query) {
        return Some(query);
    }
    if (400..450).contains(&query.0) && has(Weight::MEDIUM) {
        return Some(Weight::MEDIUM);
    }
    if (450..=500).contains(&query.0) && has(Weight::NORMAL) {
        return Some(Weight::NORMAL);
    }
    nearest(faces, |face| face.weight.0, query.0, query.0 <= 500).map(|face| face.weight)
}

/// Returns the first of `faces` whose `key` is nearest `target`, looking
/// first at keys up to `target` when `lower_first` is set, else at keys from
/// `target` up, and then at the other side.
fn nearest<'a>(
    faces: &[&'a FaceInfo],
    key: impl Fn(&FaceInfo) -> u16,
    target: u16,
    lower_first: bool,
) -> Option<&'a FaceInfo> {
    let closest_on = |lower: bool| {
        faces
            .iter()
            .copied()
            .filter(|face| (key(face) <= target) == lower || key(face) == target)
            .min_by_key(|face| key(face).abs_diff(target))
    };
    closest_on(lower_first).or_else(|| closest_on(!lower_first))
}

/// Returns the UTF-16 code unit for a Macintosh Roman byte.
#[expect(
    clippy::indexing_slicing,
    reason = "MAC_ROMAN has 256 entries, one per u8 value, so every byte indexes it"
)]
fn mac_roman_to_utf16(byte: u8) -> u16 {
    MAC_ROMAN[usize::from(byte)]
}

/// Macintosh Roman to UTF-16 encoding table.
///
/// <https://en.wikipedia.org/wiki/Mac_OS_Roman>
#[rustfmt::skip]
const MAC_ROMAN: &[u16; 256] = &[
    0x0000, 0x0001, 0x0002, 0x0003, 0x0004, 0x0005, 0x0006, 0x0007,
    0x0008, 0x0009, 0x000A, 0x000B, 0x000C, 0x000D, 0x000E, 0x000F,
    0x0010, 0x2318, 0x21E7, 0x2325, 0x2303, 0x0015, 0x0016, 0x0017,
    0x0018, 0x0019, 0x001A, 0x001B, 0x001C, 0x001D, 0x001E, 0x001F,
    0x0020, 0x0021, 0x0022, 0x0023, 0x0024, 0x0025, 0x0026, 0x0027,
    0x0028, 0x0029, 0x002A, 0x002B, 0x002C, 0x002D, 0x002E, 0x002F,
    0x0030, 0x0031, 0x0032, 0x0033, 0x0034, 0x0035, 0x0036, 0x0037,
    0x0038, 0x0039, 0x003A, 0x003B, 0x003C, 0x003D, 0x003E, 0x003F,
    0x0040, 0x0041, 0x0042, 0x0043, 0x0044, 0x0045, 0x0046, 0x0047,
    0x0048, 0x0049, 0x004A, 0x004B, 0x004C, 0x004D, 0x004E, 0x004F,
    0x0050, 0x0051, 0x0052, 0x0053, 0x0054, 0x0055, 0x0056, 0x0057,
    0x0058, 0x0059, 0x005A, 0x005B, 0x005C, 0x005D, 0x005E, 0x005F,
    0x0060, 0x0061, 0x0062, 0x0063, 0x0064, 0x0065, 0x0066, 0x0067,
    0x0068, 0x0069, 0x006A, 0x006B, 0x006C, 0x006D, 0x006E, 0x006F,
    0x0070, 0x0071, 0x0072, 0x0073, 0x0074, 0x0075, 0x0076, 0x0077,
    0x0078, 0x0079, 0x007A, 0x007B, 0x007C, 0x007D, 0x007E, 0x007F,
    0x00C4, 0x00C5, 0x00C7, 0x00C9, 0x00D1, 0x00D6, 0x00DC, 0x00E1,
    0x00E0, 0x00E2, 0x00E4, 0x00E3, 0x00E5, 0x00E7, 0x00E9, 0x00E8,
    0x00EA, 0x00EB, 0x00ED, 0x00EC, 0x00EE, 0x00EF, 0x00F1, 0x00F3,
    0x00F2, 0x00F4, 0x00F6, 0x00F5, 0x00FA, 0x00F9, 0x00FB, 0x00FC,
    0x2020, 0x00B0, 0x00A2, 0x00A3, 0x00A7, 0x2022, 0x00B6, 0x00DF,
    0x00AE, 0x00A9, 0x2122, 0x00B4, 0x00A8, 0x2260, 0x00C6, 0x00D8,
    0x221E, 0x00B1, 0x2264, 0x2265, 0x00A5, 0x00B5, 0x2202, 0x2211,
    0x220F, 0x03C0, 0x222B, 0x00AA, 0x00BA, 0x03A9, 0x00E6, 0x00F8,
    0x00BF, 0x00A1, 0x00AC, 0x221A, 0x0192, 0x2248, 0x2206, 0x00AB,
    0x00BB, 0x2026, 0x00A0, 0x00C0, 0x00C3, 0x00D5, 0x0152, 0x0153,
    0x2013, 0x2014, 0x201C, 0x201D, 0x2018, 0x2019, 0x00F7, 0x25CA,
    0x00FF, 0x0178, 0x2044, 0x20AC, 0x2039, 0x203A, 0xFB01, 0xFB02,
    0x2021, 0x00B7, 0x201A, 0x201E, 0x2030, 0x00C2, 0x00CA, 0x00C1,
    0x00CB, 0x00C8, 0x00CD, 0x00CE, 0x00CF, 0x00CC, 0x00D3, 0x00D4,
    0xF8FF, 0x00D2, 0x00DA, 0x00DB, 0x00D9, 0x0131, 0x02C6, 0x02DC,
    0x00AF, 0x02D8, 0x02D9, 0x02DA, 0x00B8, 0x02DD, 0x02DB, 0x02C7,
];

#[cfg(feature = "fs")]
pub mod env;
mod generic_family;
mod postscript_name;
#[cfg(feature = "fs")]
mod system_fonts;

// fontgres additions: configuration and CLI helpers shared by the workspace
// binaries. They need `std`, so they sit behind the `cli` feature.

#[cfg(feature = "cli")]
pub mod cli;
#[cfg(feature = "cli")]
pub mod config;
#[cfg(feature = "cli")]
pub mod error;
#[cfg(feature = "cli")]
pub mod models;

#[cfg(feature = "cli")]
pub use config::{Config, ConfigOverrides, ConfigPathSource, FontsConfig};
#[cfg(feature = "cli")]
pub use error::{ConfigError, Error};

/// The result type of the configuration and font loading in this crate, with
/// [`Error`] as the default error.
#[cfg(feature = "cli")]
pub type Result<T, E = Error> = core::result::Result<T, E>;

#[cfg(test)]
mod face_info_tests;
