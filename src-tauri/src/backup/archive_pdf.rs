//! Printable offline PDF representation of all worksheets from a frozen XLSX batch.
//! Reads archived XLSX, not live sales tables: historical exports remain consistent.
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Cursor, Read, Write};
use std::path::Path;
use calamine::{Reader, Xlsx};
use printpdf::{BuiltinFont, Mm, PdfDocument, PdfDocumentReference, PdfLayerReference};
use crate::errors::{AppError, AppResult};

const PAGE_W: f32 = 297.0;
const PAGE_H: f32 = 210.0;
const LINE_HEIGHT: f32 = 5.0;
const MAX_CHARS: usize = 118;

fn printable(value: &str) -> String {
    value.chars().map(|c| if c.is_control() { ' ' } else { c }).collect()
}
fn segments(value: &str) -> Vec<String> {
    let chars: Vec<char> = printable(value).chars().collect();
    if chars.is_empty() { return vec![String::new()]; }
    chars.chunks(MAX_CHARS).map(|s| s.iter().collect()).collect()
}

fn pdf_error(err: impl std::fmt::Display) -> AppError {
    AppError::Internal(format!("PDF generation failed: {err}"))
}
fn new_page(doc: &PdfDocumentReference, label: &str) -> PdfLayerReference {
    let (page, layer) = doc.add_page(Mm(PAGE_W), Mm(PAGE_H), label);
    doc.get_page(page).get_layer(layer)
}

/// Generate a complete, multi-page read-only representation of each XLSX
/// worksheet. Each row's columns are labeled to avoid losing columns on paper.
pub fn pdf_from_xlsx(bytes: &[u8]) -> AppResult<Vec<u8>> {
    let mut workbook: Xlsx<_> = Xlsx::new(Cursor::new(bytes.to_vec())).map_err(pdf_error)?;
    let names = workbook.sheet_names().to_vec();
    if names.is_empty() { return Err(AppError::Validation("archive workbook has no worksheets".into())); }
    let (doc, page, layer_id) = PdfDocument::new("Natra Transaction Archive", Mm(PAGE_W), Mm(PAGE_H), "Archive");
    let mut layer = doc.get_page(page).get_layer(layer_id);
    // Nyala is installed on many Windows systems and supports Ethiopic.
    // If missing, use a standard PDF font; PDFs are still generated offline.
    let ethiopic = [r"C:\Windows\Fonts\nyala.ttf", r"C:\Windows\Fonts\Nyala.ttf"];
    let mut chosen = None;
    for path in &ethiopic {
        if let Ok(file) = File::open(path) {
            if let Ok(font) = doc.add_external_font(file) { chosen = Some(font); break; }
        }
    }
    let font = match chosen {
        Some(font) => font,
        None => doc.add_builtin_font(BuiltinFont::Helvetica).map_err(pdf_error)?,
    };
    let mut y = PAGE_H - 16.0;
    let mut page_number = 1usize;
    for (sheet_index, name) in names.iter().enumerate() {
        if sheet_index > 0 {
            layer = new_page(&doc, "Archive");
            page_number += 1;
            y = PAGE_H - 16.0;
        }
        layer.use_text(format!("NATRA  /  {}", printable(name)), 15.0, Mm(12.0), Mm(y), &font);
        y -= 10.0;
        let range = workbook.worksheet_range(name).map_err(pdf_error)?;
        let mut iter = range.rows();
        let headers = iter.next().map(|r| r.iter().map(|c| c.to_string()).collect::<Vec<_>>()).unwrap_or_default();
        for (i, row) in iter.enumerate() {
            if y < 20.0 {
                layer = new_page(&doc, "Archive");
                page_number += 1;
                y = PAGE_H - 16.0;
                layer.use_text(format!("{} (continued)", printable(name)), 12.0, Mm(12.0), Mm(y), &font);
                y -= 9.0;
            }
            layer.use_text(format!("Record {}", i + 1), 9.0, Mm(12.0), Mm(y), &font);
            y -= LINE_HEIGHT;
            for (index, cell) in row.iter().enumerate() {
                let value = cell.to_string();
                if value.is_empty() { continue; }
                let title = headers.get(index).map(String::as_str).unwrap_or("Column");
                for line in segments(&format!("{}: {}", title, value)) {
                    if y < 14.0 {
                        layer = new_page(&doc, "Archive");
                        page_number += 1;
                        y = PAGE_H - 16.0;
                        layer.use_text(format!("{} (continued)", printable(name)), 12.0, Mm(12.0), Mm(y), &font);
                        y -= 9.0;
                    }
                    layer.use_text(line, 8.0, Mm(18.0), Mm(y), &font);
                    y -= LINE_HEIGHT;
                }
            }
            y -= 3.0;
        }
        if range.height() <= 1 {
            layer.use_text("No records in this worksheet.", 9.0, Mm(12.0), Mm(y), &font);
        }
    }
    let mut out = Vec::new();
    doc.save(&mut BufWriter::new(&mut out)).map_err(pdf_error)?;
    if !out.starts_with(b"%PDF-") || out.len() < 100 {
        return Err(AppError::Internal("invalid PDF output".into()));
    }
    let _ = page_number;
    Ok(out)
}

/// Save a verified PDF copy beside the user's ordinary Downloads. The XLSX
/// archive is never moved or altered, and existing PDFs are not overwritten.
pub fn save_pdf_to_downloads(archives: &Path, downloads: &Path, filename: &str) -> AppResult<String> {
    if !filename.starts_with("natra-sales-EAT-") || !filename.ends_with(".xlsx")
        || !filename.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
        || filename.contains("..") {
        return Err(AppError::Validation("invalid archive filename".into()));
    }
    let source = fs::read(archives.join(filename)).map_err(pdf_error)?;
    let pdf = pdf_from_xlsx(&source)?;
    fs::create_dir_all(downloads).map_err(pdf_error)?;
    let stem = filename.trim_end_matches(".xlsx");
    for suffix in 0..1000 {
        let name = if suffix == 0 { format!("{stem}.pdf") } else { format!("{stem}-{suffix}.pdf") };
        let dest = downloads.join(name);
        let mut file = match OpenOptions::new().write(true).create_new(true).open(&dest) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(pdf_error(e)),
        };
        let result = file.write_all(&pdf).and_then(|_| file.sync_all());
        drop(file);
        if let Err(e) = result {
            let _ = fs::remove_file(&dest);
            return Err(pdf_error(e));
        }
        return Ok(dest.to_string_lossy().into_owned());
    }
    Err(AppError::Internal("too many PDF copies in Downloads".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use crate::database::migrations;
    use crate::backup::archive_sales::build_sales_archive;

    #[test]
    fn converts_all_archive_worksheets_to_pdf() {
        let db = Connection::open_in_memory().unwrap();
        migrations::run(&db).unwrap();
        let workbook = build_sales_archive(&db, "B", "2026-10-18 21:00:00", "2026-10-19 21:00:00").unwrap();
        let pdf = pdf_from_xlsx(&workbook).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        assert!(pdf.len() > 1000);
    }
    #[test]
    fn rejects_invalid_archives() {
        assert!(pdf_from_xlsx(b"not xlsx").is_err());
    }
}
