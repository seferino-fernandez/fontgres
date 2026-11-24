//! Prints a command's result to stdout as a table, JSON or CSV.

use std::borrow::Cow;
use std::fmt;
use std::io::{self, BufWriter, ErrorKind, Write};

use serde::Serialize;
use unicode_width::UnicodeWidthStr;

/// Output format for CLI commands.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub(crate) enum OutputFormat {
    /// Human-readable table (default).
    Table,
    /// JSON (pretty-printed).
    Json,
    /// CSV with a header row.
    Csv,
}

/// The downstream reader closed stdout (e.g. `... | head`).
///
/// Output stops at the first such write, and `main` exits successfully: the
/// reader took all the output it wanted.
#[derive(Debug)]
pub(crate) struct StdoutClosed;

impl fmt::Display for StdoutClosed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("stdout was closed")
    }
}

impl std::error::Error for StdoutClosed {}

/// Prints a command's result to stdout in `format`: `data` as JSON, or the
/// `rows` under `headers` as a table or CSV, each row holding a cell per
/// header. `rows` is called only for a format that prints rows.
///
/// stdout stays locked for the whole result, which ends with a newline and is
/// buffered, so that a row is not a write of its own.
///
/// # Errors
///
/// Fails with [`StdoutClosed`] when the reader closed stdout, and with the
/// error of any other write that stdout did not take, such as one to a full
/// disk.
pub(crate) fn print<T: Serialize>(
    format: OutputFormat,
    data: &T,
    headers: &[&str],
    rows: impl FnOnce() -> Vec<Vec<String>>,
) -> anyhow::Result<()> {
    print_to(io::stdout().lock(), format, data, headers, rows)
}

/// [`print()`], to any `stream`.
fn print_to<T: Serialize>(
    stream: impl Write,
    format: OutputFormat,
    data: &T,
    headers: &[&str],
    rows: impl FnOnce() -> Vec<Vec<String>>,
) -> anyhow::Result<()> {
    let mut out = BufWriter::new(stream);
    let written = match format {
        OutputFormat::Json => write_json(&mut out, data),
        OutputFormat::Table => write_table(&mut out, headers, &rows()),
        OutputFormat::Csv => write_csv(&mut out, headers, &rows()),
    }
    .and_then(|()| out.flush());
    match written {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::BrokenPipe => Err(StdoutClosed.into()),
        Err(error) => Err(anyhow::Error::new(error).context("writing to stdout")),
    }
}

/// Writes `data` to `out` as pretty JSON.
fn write_json<T: Serialize>(out: &mut impl Write, data: &T) -> io::Result<()> {
    serde_json::to_writer_pretty(&mut *out, data)?;
    out.write_all(b"\n")
}

/// The spaces between two columns of a table.
const COLUMN_GAP: usize = 2;

/// The text of a table cell: `text` with each control character as its
/// escape, such as `\n` or `\u{1b}`, so that a line break in a name does not
/// break the line and an escape sequence does not reach the terminal.
fn table_cell(text: &str) -> Cow<'_, str> {
    if text.contains(char::is_control) {
        let escape = |character: char| -> String {
            if character.is_control() {
                character.escape_default().collect()
            } else {
                character.into()
            }
        };
        Cow::Owned(text.chars().map(escape).collect())
    } else {
        Cow::Borrowed(text)
    }
}

/// Writes `cells` to `out` as one table line, each cell but the last padded
/// to its column's width in `widths` and followed by the gap.
fn write_table_line<'a>(
    out: &mut impl Write,
    widths: &[usize],
    cells: impl IntoIterator<Item = &'a str>,
) -> io::Result<()> {
    let mut cells = cells.into_iter().zip(widths).peekable();
    while let Some((cell, width)) = cells.next() {
        let cell = table_cell(cell);
        out.write_all(cell.as_bytes())?;
        if cells.peek().is_some() {
            let padding = width
                .saturating_sub(cell.width())
                .saturating_add(COLUMN_GAP);
            write!(out, "{:padding$}", "")?;
        }
    }
    out.write_all(b"\n")
}

/// Writes `rows` to `out` as a table: a `headers` line, then a line per row,
/// with no borders. A column is as wide as its widest cell appears on a
/// terminal, where a wide character such as a kana takes two columns, and no
/// cell is wrapped or cut, so a row is always one line.
fn write_table(out: &mut impl Write, headers: &[&str], rows: &[Vec<String>]) -> io::Result<()> {
    let mut widths: Vec<usize> = headers
        .iter()
        .map(|header| table_cell(header).width())
        .collect();
    for row in rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(table_cell(cell).width());
        }
    }
    write_table_line(out, &widths, headers.iter().copied())?;
    for row in rows {
        write_table_line(out, &widths, row.iter().map(String::as_str))?;
    }
    Ok(())
}

/// Escapes a single CSV field, quoting it when necessary.
fn escape_csv_field(field: &str) -> Cow<'_, str> {
    if field.contains([',', '"', '\n']) {
        Cow::Owned(format!("\"{}\"", field.replace('"', "\"\"")))
    } else {
        Cow::Borrowed(field)
    }
}

/// Writes `fields` to `out` as one CSV line.
fn write_csv_line<'a>(
    out: &mut impl Write,
    fields: impl IntoIterator<Item = &'a str>,
) -> io::Result<()> {
    for (position, field) in fields.into_iter().enumerate() {
        if position > 0 {
            out.write_all(b",")?;
        }
        out.write_all(escape_csv_field(field).as_bytes())?;
    }
    out.write_all(b"\n")
}

/// Writes `rows` to `out` as CSV under a `headers` line.
fn write_csv(out: &mut impl Write, headers: &[&str], rows: &[Vec<String>]) -> io::Result<()> {
    write_csv_line(out, headers.iter().copied())?;
    for row in rows {
        write_csv_line(out, row.iter().map(String::as_str))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rows of a two-column listing.
    fn rows(rows: &[[&str; 2]]) -> Vec<Vec<String>> {
        rows.iter()
            .map(|row| row.iter().map(|&cell| cell.to_owned()).collect())
            .collect()
    }

    /// What [`print_to`] writes for `rows` under two headers in `format`.
    fn printed(format: OutputFormat, listed: &[[&str; 2]]) -> String {
        let mut stream = Vec::new();
        print_to(&mut stream, format, &listed, &["Family", "Weight"], || {
            rows(listed)
        })
        .unwrap();
        String::from_utf8(stream).unwrap()
    }

    #[test]
    fn escape_csv_plain() {
        assert_eq!(escape_csv_field("hello"), "hello");
    }

    #[test]
    fn escape_csv_with_comma() {
        assert_eq!(escape_csv_field("hello,world"), "\"hello,world\"");
    }

    #[test]
    fn escape_csv_with_quotes() {
        assert_eq!(escape_csv_field(r#"say "hi""#), r#""say ""hi""""#);
    }

    #[test]
    fn escape_csv_with_newline() {
        assert_eq!(escape_csv_field("line1\nline2"), "\"line1\nline2\"");
    }

    #[test]
    fn escape_csv_with_comma_and_quotes() {
        assert_eq!(escape_csv_field(r#"a,"b""#), r#""a,""b""""#);
    }

    #[test]
    fn csv_is_a_header_line_then_a_line_per_row() {
        assert_eq!(
            printed(OutputFormat::Csv, &[["Tuffy", "500"], ["A, B", "7\"0"]]),
            "Family,Weight\nTuffy,500\n\"A, B\",\"7\"\"0\"\n"
        );
        assert_eq!(printed(OutputFormat::Csv, &[]), "Family,Weight\n");
    }

    #[test]
    fn json_is_the_data_pretty_printed_and_reads_no_rows() {
        let mut stream = Vec::new();
        let printed = print_to(&mut stream, OutputFormat::Json, &["Tuffy"], &[], || {
            panic!("JSON prints no rows")
        });
        printed.unwrap();
        assert_eq!(stream, b"[\n  \"Tuffy\"\n]\n");
    }

    #[test]
    fn table_is_a_header_line_then_an_aligned_line_per_row() {
        assert_eq!(
            printed(
                OutputFormat::Table,
                &[["Tuffy", "500"], ["Tuffy Condensed", "90"]]
            ),
            "Family           Weight\n\
             Tuffy            500\n\
             Tuffy Condensed  90\n"
        );
        assert_eq!(printed(OutputFormat::Table, &[]), "Family  Weight\n");
    }

    /// A wide character takes two columns of a terminal, so it counts as two.
    #[test]
    fn table_columns_align_by_display_width() {
        assert_eq!(
            printed(
                OutputFormat::Table,
                &[["游ゴシック体", "400"], ["Tuffy", "500"]]
            ),
            "Family        Weight\n\
             游ゴシック体  400\n\
             Tuffy         500\n"
        );
    }

    /// A line break or an escape sequence in a name would break the line or
    /// reach the terminal: the table shows each as text.
    #[test]
    fn table_shows_a_control_character_as_its_escape() {
        assert_eq!(
            printed(
                OutputFormat::Table,
                &[["Tuf\nfy\u{1b}[1m", "500"], ["Tuffy\u{85}", "90\t"]]
            ),
            "Family            Weight\n\
             Tuf\\nfy\\u{1b}[1m  500\n\
             Tuffy\\u{85}       90\\t\n"
        );
    }

    /// A stream that counts the writes it receives, or fails each one.
    struct Stream {
        writes: usize,
        fails_with: Option<ErrorKind>,
    }

    impl Write for Stream {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.writes = self.writes.saturating_add(1);
            match self.fails_with {
                Some(kind) => Err(kind.into()),
                None => Ok(bytes.len()),
            }
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// Prints a listing of `length` rows to `stream` in `format`. A hundred
    /// rows fit the buffer and a thousand overflow it in every format, so a
    /// failing stream fails the last flush of the first and a write in the
    /// middle of the second.
    fn print_a_listing(
        stream: &mut Stream,
        format: OutputFormat,
        length: usize,
    ) -> anyhow::Result<()> {
        let listed = vec![["Tuffy Condensed", "500"]; length];
        print_to(stream, format, &listed, &["Family", "Weight"], || {
            rows(&listed)
        })
    }

    /// The error of printing a short and a long listing in each format to a
    /// stream whose writes fail with `kind`.
    fn errors_of_printing_to_a_stream_failing_with(kind: ErrorKind) -> Vec<anyhow::Error> {
        let formats = [OutputFormat::Table, OutputFormat::Json, OutputFormat::Csv];
        let mut errors = Vec::new();
        for format in formats {
            for length in [100, 1000] {
                let mut stream = Stream {
                    writes: 0,
                    fails_with: Some(kind),
                };
                errors.push(print_a_listing(&mut stream, format, length).unwrap_err());
            }
        }
        errors
    }

    #[test]
    fn the_rows_of_a_listing_reach_the_stream_in_one_write() {
        let mut stream = Stream {
            writes: 0,
            fails_with: None,
        };
        print_a_listing(&mut stream, OutputFormat::Csv, 100).unwrap();
        assert_eq!(stream.writes, 1);
    }

    #[test]
    fn a_closed_stream_stops_the_output() {
        for error in errors_of_printing_to_a_stream_failing_with(ErrorKind::BrokenPipe) {
            assert!(error.is::<StdoutClosed>(), "{error:#}");
        }
    }

    #[test]
    fn a_stream_that_rejects_writes_is_an_error_naming_stdout() {
        for error in errors_of_printing_to_a_stream_failing_with(ErrorKind::StorageFull) {
            assert!(!error.is::<StdoutClosed>());
            assert_eq!(
                format!("{error:#}"),
                format!(
                    "writing to stdout: {}",
                    io::Error::from(ErrorKind::StorageFull)
                )
            );
        }
    }
}
