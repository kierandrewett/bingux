//! Files to preview, written byte by byte: a PDF and a WAV, so the tests of the
//! host, of the viewers and the taxonomy's fixture world need no tool to make
//! their inputs. (A film needs an encoder; `gst-launch-1.0` makes those.)

/// A PDF of Letter pages. Each page is a list of lines: the text and where its
/// baseline starts, in points from the page's bottom left, set in 24 pt
/// Helvetica.
pub fn pdf(pages: &[&[(&str, i32, i32)]]) -> Vec<u8> {
    pdf_sized((612, 792), pages)
}

/// [`pdf`] with pages of `size` points (wide, tall) instead of Letter.
pub fn pdf_sized(size: (i32, i32), pages: &[&[(&str, i32, i32)]]) -> Vec<u8> {
    // 1 catalog, 2 pages, 3 font, then a page and its content for each page.
    let mut objects: Vec<String> = vec!["<< /Type /Catalog /Pages 2 0 R >>".into()];
    let kids: Vec<String> = (0..pages.len()).map(|n| format!("{} 0 R", 4 + n * 2)).collect();
    objects.push(format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), pages.len()));
    objects.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into());
    for (n, lines) in pages.iter().enumerate() {
        let content: String = lines.iter().map(|(text, x, y)| format!("BT /F1 24 Tf {x} {y} Td ({}) Tj ET\n", escape(text))).collect();
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
            size.0,
            size.1,
            5 + n * 2
        ));
        objects.push(format!("<< /Length {} >>\nstream\n{}endstream", content.len(), content));
    }
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", index + 1, body).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for offset in &offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
    out
}

/// One entry of a PDF's outline: where it goes (a page, from 0) and its
/// sections.
pub struct Chapter<'a> {
    pub title: &'a str,
    pub page: usize,
    pub sections: Vec<Chapter<'a>>,
}

impl<'a> Chapter<'a> {
    pub fn new(title: &'a str, page: usize) -> Self {
        Chapter { title, page, sections: Vec::new() }
    }

    pub fn with(mut self, section: Chapter<'a>) -> Self {
        self.sections.push(section);
        self
    }

    fn count(&self) -> usize {
        self.sections.iter().map(|section| 1 + section.count()).sum()
    }
}

/// [`pdf`] with an outline (a table of contents) built from `chapters`.
pub fn pdf_with_outline(pages: &[&[(&str, i32, i32)]], chapters: &[Chapter]) -> Vec<u8> {
    let mut objects: Vec<String> = vec![String::new()]; // the catalogue, filled in below
    let kids: Vec<String> = (0..pages.len()).map(|n| format!("{} 0 R", 4 + n * 2)).collect();
    objects.push(format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), pages.len()));
    objects.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into());
    for (n, lines) in pages.iter().enumerate() {
        let content: String = lines.iter().map(|(text, x, y)| format!("BT /F1 24 Tf {x} {y} Td ({}) Tj ET\n", escape(text))).collect();
        objects.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>", 5 + n * 2));
        objects.push(format!("<< /Length {} >>\nstream\n{}endstream", content.len(), content));
    }
    // The outline root, then every entry depth first.
    let root = objects.len() + 1;
    objects.push(String::new());
    let first_item = objects.len() + 1;
    let total: usize = chapters.iter().map(|chapter| 1 + chapter.count()).sum();
    for _ in 0..total {
        objects.push(String::new());
    }
    fn fill(objects: &mut [String], level: &[Chapter], parent: usize, next_free: &mut usize) -> (Option<usize>, Option<usize>) {
        let ids: Vec<usize> = level
            .iter()
            .map(|chapter| {
                let id = *next_free;
                *next_free += 1 + chapter.count();
                id
            })
            .collect();
        for (index, (chapter, id)) in level.iter().zip(&ids).enumerate() {
            let mut inner = *id + 1;
            let (first, last) = fill(objects, &chapter.sections, *id, &mut inner);
            let mut entry = format!("<< /Title ({}) /Parent {parent} 0 R /Dest [{} 0 R /Fit]", escape(chapter.title), 4 + chapter.page * 2);
            if index > 0 {
                entry.push_str(&format!(" /Prev {} 0 R", ids[index - 1]));
            }
            if index + 1 < ids.len() {
                entry.push_str(&format!(" /Next {} 0 R", ids[index + 1]));
            }
            if let (Some(first), Some(last)) = (first, last) {
                entry.push_str(&format!(" /First {first} 0 R /Last {last} 0 R /Count {}", chapter.count()));
            }
            entry.push_str(" >>");
            objects[*id - 1] = entry;
        }
        (ids.first().copied(), ids.last().copied())
    }
    let mut next_free = first_item;
    let (first, last) = fill(&mut objects, chapters, root, &mut next_free);
    objects[root - 1] = match (first, last) {
        (Some(first), Some(last)) => format!("<< /Type /Outlines /First {first} 0 R /Last {last} 0 R /Count {total} >>"),
        _ => "<< /Type /Outlines /Count 0 >>".to_owned(),
    };
    objects[0] = format!("<< /Type /Catalog /Pages 2 0 R /Outlines {root} 0 R /PageMode /UseOutlines >>");
    write_pdf(&objects)
}

/// A report of `pages` pages of text, each headed "Chapter N" with some
/// paragraphs under it, and an outline of one chapter per page with two
/// sections in the first: a document with something to scroll, search,
/// select and jump around, for a screenshot or a test.
pub fn report_pdf(pages: usize) -> Vec<u8> {
    let words = ["analysis", "budget", "delivery", "forecast", "quarter", "revenue", "summary", "schedule"];
    let owned: Vec<Vec<(String, i32, i32)>> = (0..pages)
        .map(|page| {
            let mut lines = vec![(format!("Chapter {}", page + 1), 72, 720)];
            for row in 0..14 {
                let line: Vec<&str> = (0..4).map(|n| words[(page * 3 + row + n * 5) % words.len()]).collect();
                lines.push((line.join(" "), 72, 680 - row as i32 * 34));
            }
            lines
        })
        .collect();
    let refs: Vec<Vec<(&str, i32, i32)>> = owned.iter().map(|lines| lines.iter().map(|(text, x, y)| (text.as_str(), *x, *y)).collect()).collect();
    let slices: Vec<&[(&str, i32, i32)]> = refs.iter().map(Vec::as_slice).collect();
    let titles: Vec<String> = (0..pages).map(|page| format!("Chapter {}", page + 1)).collect();
    let chapters: Vec<Chapter> = titles
        .iter()
        .enumerate()
        .map(|(page, title)| {
            let chapter = Chapter::new(title, page);
            if page == 0 {
                chapter.with(Chapter::new("Background", 0)).with(Chapter::new("Method", pages.min(2).saturating_sub(1)))
            } else {
                chapter
            }
        })
        .collect();
    pdf_with_outline(&slices, &chapters)
}

/// Objects as a PDF file with a cross-reference table.
fn write_pdf(objects: &[String]) -> Vec<u8> {
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", index + 1, body).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for offset in &offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
    out
}

/// The parentheses and backslash of a PDF string.
fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)")
}

/// Three pages with text to search for: "Invoice 105" and "Total due 40.00" on
/// the first, "Second page" on the second, "Last page invoice" on the third.
pub fn three_page_pdf() -> Vec<u8> {
    pdf(&[
        &[("Invoice 105", 72, 720), ("Total due 40.00", 72, 100)],
        &[("Second page", 72, 720)],
        &[("Last page invoice", 72, 720)],
    ])
}

/// A mono 16-bit WAV of a 440 Hz sine, `seconds` long.
pub fn wav_tone(seconds: u32) -> Vec<u8> {
    let rate = 22_050u32;
    let samples: Vec<i16> = (0..rate * seconds).map(|n| ((n as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin() * 8000.0) as i16).collect();
    let data_len = samples.len() as u32 * 2;
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pdf_has_the_pages_and_a_cross_reference_table_that_points_at_its_objects() {
        let bytes = three_page_pdf();
        let text = String::from_utf8(bytes.clone()).unwrap();
        assert!(text.starts_with("%PDF-1.4\n") && text.ends_with("%%EOF\n"));
        assert!(text.contains("/Count 3"));
        let startxref: usize = text.rsplit("startxref\n").next().unwrap().lines().next().unwrap().parse().unwrap();
        assert!(text[startxref..].starts_with("xref\n"));
        // Every offset in the table lands on "N 0 obj".
        let table: Vec<&str> = text[startxref..].lines().skip(3).take(9).collect();
        for (index, line) in table.iter().enumerate() {
            let offset: usize = line[..10].parse().unwrap();
            assert!(text[offset..].starts_with(&format!("{} 0 obj", index + 1)), "{line}");
        }
    }

    #[test]
    fn the_wav_is_the_length_it_says() {
        let bytes = wav_tone(2);
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(bytes.len(), 44 + 22_050 * 2 * 2);
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), 22_050 * 2 * 2);
    }
}
