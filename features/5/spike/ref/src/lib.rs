//! Spike helpers: minimal PDF-1.4 fixture generators, std only.

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            '\\' => out.push_str("\\\\"),
            _ => out.push(c),
        }
    }
    out
}

/// Build a single-page PDF-1.4 with the given raw content stream bytes.
/// `with_image` adds a 1x1 gray uncompressed image XObject `/Im1` to `/Resources`.
pub fn build_pdf(content: &[u8], with_image: bool) -> Vec<u8> {
    let resources = if with_image {
        "<< /Font << /F1 5 0 R >> /XObject << /Im1 6 0 R >> >>"
    } else {
        "<< /Font << /F1 5 0 R >> >>"
    };
    let mut objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources {resources} /Contents 4 0 R >>"
        )
        .into_bytes(),
    ];
    let mut content_obj = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
    content_obj.extend_from_slice(content);
    content_obj.extend_from_slice(b"\nendstream");
    objects.push(content_obj);
    objects.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec());
    if with_image {
        let mut img = b"<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length 1 >>\nstream\n".to_vec();
        img.push(0x80);
        img.extend_from_slice(b"\nendstream");
        objects.push(img);
    }
    assemble(objects)
}

fn assemble(objects: Vec<Vec<u8>>) -> Vec<u8> {
    let mut buf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(buf.len());
        buf.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        buf.extend_from_slice(obj);
        buf.extend_from_slice(b"\nendobj\n");
    }
    let xref_offset = buf.len();
    let n = objects.len() + 1;
    buf.extend_from_slice(format!("xref\n0 {n}\n0000000000 65535 f \n").as_bytes());
    for off in &offsets {
        buf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    buf.extend_from_slice(
        format!("trailer\n<< /Size {n} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF")
            .as_bytes(),
    );
    buf
}

/// Emit a valid single-page PDF-1.4 that renders `lines` top-down with base-14 Helvetica.
pub fn minimal_pdf(lines: &[&str]) -> Vec<u8> {
    let mut content = String::from("BT /F1 12 Tf 72 720 Td ");
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            content.push_str("0 -16 Td ");
        }
        content.push_str(&format!("({}) Tj ", escape(line)));
    }
    content.push_str("ET");
    build_pdf(content.as_bytes(), false)
}

/// Page with a tiny 1x1 gray image XObject drawn via `/Im1 Do`, no text.
pub fn image_only_pdf() -> Vec<u8> {
    build_pdf(b"q 100 0 0 100 0 0 cm /Im1 Do Q", true)
}

/// Same page/content as `minimal_pdf` but `/Resources` omits `/Font` even though
/// the content stream still does `/F1 12 Tf` (Q1f).
pub fn pdf_missing_font_resource(lines: &[&str]) -> Vec<u8> {
    let mut content = String::from("BT /F1 12 Tf 72 720 Td ");
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            content.push_str("0 -16 Td ");
        }
        content.push_str(&format!("({}) Tj ", escape(line)));
    }
    content.push_str("ET");
    let objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << >> /Contents 4 0 R >>"
            .to_vec(),
        {
            let mut c = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
            c.extend_from_slice(content.as_bytes());
            c.extend_from_slice(b"\nendstream");
            c
        },
    ];
    assemble(objects)
}

/// Corrupt the `startxref` byte offset in an otherwise valid PDF (Q1e).
pub fn corrupt_startxref(pdf: &[u8]) -> Vec<u8> {
    let marker = b"startxref\n";
    let pos = pdf
        .windows(marker.len())
        .position(|w| w == marker)
        .unwrap_or(0);
    let mut out = pdf.to_vec();
    let num_start = pos + marker.len();
    if let Some(rel_end) = out[num_start..].iter().position(|&b| b == b'\n') {
        for b in &mut out[num_start..num_start + rel_end] {
            *b = b'9';
        }
    }
    out
}
