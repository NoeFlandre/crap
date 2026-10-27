//! Output formats of `crap report`.

use std::cmp::Ordering;
use std::io::{self, Write};

use colored::Colorize;

use crap_linter::{FileReport, FunctionReport};

use crate::args::{ReportArgs, ReportFormat, SortKey};
use crate::commands::Run;
use crate::logging::{LogLevel, level};
use crate::printer::{coverage, function_json, number, score};

pub(crate) struct Reporter<'a> {
    run: &'a Run,
    args: &'a ReportArgs,
}

struct Row<'a> {
    file: &'a FileReport,
    function: &'a FunctionReport,
    location: String,
}

impl<'a> Reporter<'a> {
    pub(crate) fn new(run: &'a Run, args: &'a ReportArgs) -> Self {
        Self { run, args }
    }

    fn rows(&self) -> (Vec<Row<'a>>, usize) {
        let threshold = self.run.settings.threshold;
        let mut rows: Vec<Row<'a>> = self
            .run
            .analysis
            .functions()
            .filter(|(_, function)| {
                if self.args.crappy_only {
                    function.is_crappy(threshold)
                } else {
                    self.args
                        .min_score
                        .is_none_or(|min_score| function.score >= min_score)
                }
            })
            .map(|(file, function)| Row {
                file,
                function,
                location: format!("{}:{}", self.run.display_path(&file.path), function.line),
            })
            .collect();

        let by_location = |a: &Row<'_>, b: &Row<'_>| {
            a.file
                .path
                .cmp(&b.file.path)
                .then(a.function.line.cmp(&b.function.line))
        };
        let descending = |a: f64, b: f64| b.partial_cmp(&a).unwrap_or(Ordering::Equal);
        match self.args.sort {
            SortKey::Score => rows.sort_by(|a, b| {
                descending(a.function.score, b.function.score).then_with(|| by_location(a, b))
            }),
            SortKey::Complexity => rows.sort_by(|a, b| {
                b.function
                    .complexity
                    .cmp(&a.function.complexity)
                    .then_with(|| descending(a.function.score, b.function.score))
                    .then_with(|| by_location(a, b))
            }),
            SortKey::Coverage => rows.sort_by(|a, b| {
                a.function
                    .coverage()
                    .partial_cmp(&b.function.coverage())
                    .unwrap_or(Ordering::Equal)
                    .then_with(|| descending(a.function.score, b.function.score))
                    .then_with(|| by_location(a, b))
            }),
            SortKey::Location => rows.sort_by(by_location),
            SortKey::Name => rows.sort_by(|a, b| {
                a.function
                    .qualified_name
                    .cmp(&b.function.qualified_name)
                    .then_with(|| by_location(a, b))
            }),
        }

        let matching = rows.len();
        if let Some(top) = self.args.top {
            rows.truncate(top);
        }
        (rows, matching)
    }

    pub(crate) fn write(&self, writer: &mut dyn Write) -> io::Result<()> {
        let (rows, matching) = self.rows();
        match self.args.output_format {
            ReportFormat::Table => self.write_table(writer, &rows, matching),
            ReportFormat::Markdown => self.write_markdown(writer, &rows),
            ReportFormat::Csv => Self::write_csv(writer, &rows, self.run.settings.threshold),
            ReportFormat::Json => {
                let values: Vec<_> = rows
                    .iter()
                    .map(|row| function_json(self.run, row.file, row.function))
                    .collect();
                serde_json::to_writer_pretty(&mut *writer, &values)?;
                writeln!(writer)
            }
        }
    }

    fn write_table(
        &self,
        writer: &mut dyn Write,
        rows: &[Row<'_>],
        matching: usize,
    ) -> io::Result<()> {
        let threshold = self.run.settings.threshold;
        let total = self.run.analysis.functions().count();

        if rows.is_empty() {
            if level() >= LogLevel::Default {
                writeln!(writer, "No functions to report.")?;
            }
            return Ok(());
        }

        let headers = ["CRAP", "Complexity", "Coverage", "Function", "Location"];
        let cells: Vec<[String; 5]> = rows
            .iter()
            .map(|row| {
                [
                    score(row.function.score),
                    row.function.complexity.to_string(),
                    coverage(row.function),
                    row.function.qualified_name.clone(),
                    row.location.clone(),
                ]
            })
            .collect();
        let mut widths = headers.map(str::len);
        for row in &cells {
            for (width, cell) in widths.iter_mut().zip(row) {
                *width = (*width).max(cell.chars().count());
            }
        }

        let header = format!(
            "{:>w0$}  {:>w1$}  {:>w2$}  {:<w3$}  {}",
            headers[0],
            headers[1],
            headers[2],
            headers[3],
            headers[4],
            w0 = widths[0],
            w1 = widths[1],
            w2 = widths[2],
            w3 = widths[3],
        );
        writeln!(writer, "{}", header.bold())?;
        for (row, cell) in rows.iter().zip(&cells) {
            let padded_score = format!("{:>w$}", cell[0], w = widths[0]);
            let styled_score = if row.function.is_crappy(threshold) {
                padded_score.red().bold()
            } else {
                padded_score.green()
            };
            writeln!(
                writer,
                "{styled_score}  {:>w1$}  {:>w2$}  {:<w3$}  {}",
                cell[1],
                cell[2],
                cell[3],
                cell[4].dimmed(),
                w1 = widths[1],
                w2 = widths[2],
                w3 = widths[3],
            )?;
        }

        if level() >= LogLevel::Default {
            let crappy = self
                .run
                .analysis
                .functions()
                .filter(|(_, function)| function.is_crappy(threshold))
                .count();
            writeln!(writer)?;
            let shown = if rows.len() < matching {
                format!("Showing {} of {matching} matching functions. ", rows.len())
            } else {
                String::new()
            };
            let noun = if total == 1 { "function" } else { "functions" };
            writeln!(
                writer,
                "{shown}{total} {noun} analyzed, {} above the threshold of {}.",
                crappy.to_string().bold(),
                number(threshold)
            )?;
        }
        Ok(())
    }

    fn write_markdown(&self, writer: &mut dyn Write, rows: &[Row<'_>]) -> io::Result<()> {
        let threshold = self.run.settings.threshold;
        writeln!(
            writer,
            "| CRAP | Complexity | Coverage | Function | Location |"
        )?;
        writeln!(writer, "| ---: | ---: | ---: | :--- | :--- |")?;
        for row in rows {
            let score = if row.function.is_crappy(threshold) {
                format!("**{}**", score(row.function.score))
            } else {
                score(row.function.score)
            };
            writeln!(
                writer,
                "| {score} | {} | {} | `{}` | `{}` |",
                row.function.complexity,
                coverage(row.function),
                row.function.qualified_name,
                row.location.replace('\\', "/"),
            )?;
        }
        Ok(())
    }

    fn write_csv(writer: &mut dyn Write, rows: &[Row<'_>], threshold: f64) -> io::Result<()> {
        writeln!(
            writer,
            "path,line,function,complexity,coverage,statements,covered,score,crappy"
        )?;
        for row in rows {
            let path = row
                .location
                .rsplit_once(':')
                .map_or(row.location.as_str(), |(path, _)| path);
            writeln!(
                writer,
                "{},{},{},{},{:.4},{},{},{:.4},{}",
                csv_field(path),
                row.function.line,
                csv_field(&row.function.qualified_name),
                row.function.complexity,
                row.function.coverage(),
                row.function.statements,
                row.function.covered,
                row.function.score,
                row.function.is_crappy(threshold),
            )?;
        }
        Ok(())
    }
}

fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}
