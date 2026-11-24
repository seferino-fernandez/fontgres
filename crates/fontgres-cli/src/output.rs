//! Prints a command's result to stdout as a table, JSON or CSV.

use std::borrow::Cow;
use std::fmt;
use std::io::{self, BufWriter, ErrorKind, Write};

use comfy_table::{ContentArrangement, Table};
use serde::Serialize;

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
/// `rows` under `headers` as a table or CSV. `rows` is called only for a
/// format that prints rows.
///
/// stdout stays locked for the whole result, which ends with a newline and is
/// buffered, so that a row is not a write of its own.
///
/// # Errors
///
/// Fails with [`StdoutClosed`] when the reader closed stdout, without which a
/// `println!` would panic. Other write errors are ignored (best-effort
/// terminal output).
pub(crate) fn print<T: Serialize>(
    format: OutputFormat,
    data: &T,
    headers: &[&str],
    rows: impl FnOnce() -> Vec<Vec<String>>,
) -> Result<(), StdoutClosed> {
    print_to(io::stdout().lock(), format, data, headers, rows)
}

/// [`print()`], to any `stream`.
fn print_to<T: Serialize>(
    stream: impl Write,
    format: OutputFormat,
    data: &T,
    headers: &[&str],
    rows: impl FnOnce() -> Vec<Vec<String>>,
) -> Result<(), StdoutClosed> {
    let mut out = BufWriter::new(stream);
    let written = match format {
        OutputFormat::Json => write_json(&mut out, data),
        OutputFormat::Table => write_table(&mut out, headers, &rows()),
        OutputFormat::Csv => write_csv(&mut out, headers, &rows()),
    }
    .and_then(|()| out.flush());
    match written {
        Err(error) if error.kind() == ErrorKind::BrokenPipe => Err(StdoutClosed),
        Ok(()) | Err(_) => Ok(()),
    }
}

/// Writes `data` to `out` as pretty JSON.
fn write_json<T: Serialize>(out: &mut impl Write, data: &T) -> io::Result<()> {
    serde_json::to_writer_pretty(&mut *out, data)?;
    out.write_all(b"\n")
}

/// Writes `rows` to `out` as a [`comfy_table`] table under `headers`.
fn write_table(out: &mut impl Write, headers: &[&str], rows: &[Vec<String>]) -> io::Result<()> {
    let mut table = Table::new();
    table.set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(headers);
    for row in rows {
        table.add_row(row);
    }
    writeln!(out, "{table}")
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
    fn table_is_a_bordered_grid_under_the_headers() {
        assert_eq!(
            printed(OutputFormat::Table, &[["Tuffy", "500"]]),
            "+--------+--------+\n\
             | Family | Weight |\n\
             +=================+\n\
             | Tuffy  | 500    |\n\
             +--------+--------+\n"
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

    /// Prints a hundred-row CSV listing, about 2 KB, to `stream`.
    fn print_a_listing(stream: &mut Stream) -> Result<(), StdoutClosed> {
        let listed = vec![["Tuffy Condensed", "500"]; 100];
        print_to(
            stream,
            OutputFormat::Csv,
            &(),
            &["Family", "Weight"],
            || rows(&listed),
        )
    }

    #[test]
    fn the_rows_of_a_listing_reach_the_stream_in_one_write() {
        let mut stream = Stream {
            writes: 0,
            fails_with: None,
        };
        print_a_listing(&mut stream).unwrap();
        assert_eq!(stream.writes, 1);
    }

    #[test]
    fn a_closed_stream_stops_the_output() {
        let mut stream = Stream {
            writes: 0,
            fails_with: Some(ErrorKind::BrokenPipe),
        };
        assert!(print_a_listing(&mut stream).is_err());
    }
}
