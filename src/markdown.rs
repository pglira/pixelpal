//! Markdown of the chat replies. egui_commonmark draws the text; the tables
//! are drawn here, with columns that share the available width and cells
//! whose text wraps, so that a wide table never makes the panel wider.

use egui::{Frame, Ui};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

/// A part of a Markdown text: plain Markdown, or a table.
#[derive(Debug, PartialEq)]
enum Block {
    Text(String),
    Table {
        header: Vec<String>,
        rows: Vec<Vec<String>>,
    },
}

/// Shortest column width, in points.
const MIN_COLUMN: f32 = 40.0;

/// Draws the Markdown text `text`.
pub fn show(ui: &mut Ui, cache: &mut CommonMarkCache, text: &str) {
    for (i, block) in split(text).into_iter().enumerate() {
        ui.push_id(i, |ui| match block {
            Block::Text(t) => {
                CommonMarkViewer::new().show(ui, cache, &t);
            }
            Block::Table { header, rows } => table(ui, cache, &header, &rows),
        });
    }
}

fn table(ui: &mut Ui, cache: &mut CommonMarkCache, header: &[String], rows: &[Vec<String>]) {
    let n = header.len();
    let spacing = ui.spacing().item_spacing.x;
    Frame::group(ui.style()).show(ui, |ui| {
        let avail = (ui.available_width() - spacing * (n.saturating_sub(1)) as f32).max(0.0);
        let widths = column_widths(header, rows, avail);
        egui::Grid::new("table")
            .striped(true)
            .spacing([spacing, 4.0])
            .show(ui, |ui| {
                for row in std::iter::once(header).chain(rows.iter().map(Vec::as_slice)) {
                    for (c, w) in widths.iter().enumerate() {
                        let cell = row.get(c).map_or("", String::as_str);
                        ui.vertical(|ui| {
                            ui.set_width(*w);
                            ui.push_id(c, |ui| {
                                CommonMarkViewer::new().show(ui, cache, cell);
                            });
                        });
                    }
                    ui.end_row();
                }
            });
    });
}

/// Widths of the columns: shares of `avail` that follow the length of the
/// longest cell of each column, each at least `MIN_COLUMN`.
fn column_widths(header: &[String], rows: &[Vec<String>], avail: f32) -> Vec<f32> {
    let weights: Vec<f32> = (0..header.len())
        .map(|c| {
            let longest = std::iter::once(header)
                .chain(rows.iter().map(Vec::as_slice))
                .filter_map(|r| r.get(c))
                .map(|s| s.chars().count())
                .max()
                .unwrap_or(0);
            (longest as f32).clamp(4.0, 60.0)
        })
        .collect();
    let total: f32 = weights.iter().sum::<f32>().max(1.0);
    weights
        .iter()
        .map(|w| (avail * w / total).max(MIN_COLUMN))
        .collect()
}

/// Splits `text` into Markdown parts and tables. A table is a header line
/// with `|`, a delimiter line such as `|---|:--:|`, and the lines with `|`
/// that follow. Lines in code fences are never tables.
fn split(text: &str) -> Vec<Block> {
    let lines: Vec<&str> = text.lines().collect();
    let mut blocks = Vec::new();
    let mut plain = String::new();
    let mut fence = false;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
            fence = !fence;
        }
        let header = (!fence && line.contains('|'))
            .then(|| cells(line))
            .filter(|h| !h.is_empty());
        if let Some(header) = header
            && lines
                .get(i + 1)
                .is_some_and(|l| is_delimiter(l, header.len()))
        {
            if !plain.is_empty() {
                blocks.push(Block::Text(std::mem::take(&mut plain)));
            }
            let mut rows = Vec::new();
            i += 2;
            while let Some(l) = lines
                .get(i)
                .filter(|l| l.contains('|') && !l.trim().is_empty())
            {
                rows.push(cells(l));
                i += 1;
            }
            blocks.push(Block::Table { header, rows });
            continue;
        }
        plain.push_str(line);
        plain.push('\n');
        i += 1;
    }
    if !plain.is_empty() {
        blocks.push(Block::Text(plain));
    }
    blocks
}

/// The cells of a table line: split at `|` outside of code spans, without
/// the outer pipes. `\|` is a literal pipe.
fn cells(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cell = String::new();
    let mut code = false;
    let mut chars = line.trim().chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' if chars.peek() == Some(&'|') => {
                cell.push('|');
                chars.next();
            }
            '`' => {
                code = !code;
                cell.push(ch);
            }
            '|' if !code => out.push(std::mem::take(&mut cell)),
            _ => cell.push(ch),
        }
    }
    out.push(cell);
    let t = line.trim();
    if t.starts_with('|') {
        out.remove(0);
    }
    if t.ends_with('|') && !t.ends_with("\\|") && out.len() > 1 {
        out.pop();
    }
    out.into_iter().map(|c| c.trim().to_owned()).collect()
}

/// True if `line` is the delimiter line of a table with `n` columns.
fn is_delimiter(line: &str, n: usize) -> bool {
    let c = cells(line);
    c.len() == n
        && c.iter().all(|cell| {
            let d = cell.trim_matches(':');
            !d.is_empty() && d.chars().all(|ch| ch == '-')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_table_is_split_from_the_text() {
        let text = "Intro\n\n| A | B |\n|---|:-:|\n| 1 | `x|y` |\n| 2 | a \\| b |\n\nEnd\n";
        let blocks = split(text);
        assert_eq!(
            blocks,
            vec![
                Block::Text("Intro\n\n".into()),
                Block::Table {
                    header: vec!["A".into(), "B".into()],
                    rows: vec![
                        vec!["1".into(), "`x|y`".into()],
                        vec!["2".into(), "a | b".into()],
                    ],
                },
                Block::Text("\nEnd\n".into()),
            ]
        );
    }

    #[test]
    fn pipes_in_code_fences_are_text() {
        let text = "```\n| A | B |\n|---|---|\n```\n";
        assert_eq!(split(text), vec![Block::Text(text.into())]);
    }

    #[test]
    fn columns_share_the_width() {
        let header = vec!["Name".to_owned(), "Description".to_owned()];
        let rows = vec![vec!["x".to_owned(), "a".repeat(100)]];
        let w = column_widths(&header, &rows, 600.0);
        assert!(w[0] >= MIN_COLUMN && w[1] > w[0]);
        assert!(w.iter().sum::<f32>() <= 600.0 + MIN_COLUMN);
    }
}
