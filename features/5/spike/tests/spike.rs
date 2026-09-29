use pdf_extract::extract_text_from_mem;
use spike_5::{build_pdf, corrupt_startxref, image_only_pdf, minimal_pdf, pdf_missing_font_resource};
use std::panic::{catch_unwind, AssertUnwindSafe};

fn report(label: &str, input: &[u8]) {
    let result = catch_unwind(AssertUnwindSafe(|| extract_text_from_mem(input)));
    match result {
        Ok(Ok(s)) => println!("Q1 {label}: Ok({s:?})"),
        Ok(Err(e)) => println!("Q1 {label}: Err({e:?})"),
        Err(payload) => {
            let msg = payload
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "<non-string panic payload>".to_string());
            println!("Q1 {label}: PANIC({msg})");
        }
    }
}

#[test]
fn q1_corrupt_inputs() {
    report("a_fake_header", b"%PDF-1.4 fake");
    report("b_empty", b"");
    report("c_random_64", &[0x37u8; 64]);

    let good = minimal_pdf(&["Total: 1,234.56 EUR", "Due date: 15/10/2026"]);
    let half = &good[..good.len() / 2];
    let ninety = &good[..(good.len() * 9) / 10];
    report("d_truncated_50pct", half);
    report("d_truncated_90pct", ninety);

    let bad_xref = corrupt_startxref(&good);
    report("e_corrupt_startxref", &bad_xref);

    let missing_font = pdf_missing_font_resource(&["Total: 1,234.56 EUR"]);
    report("f_missing_font_resource", &missing_font);
}

#[test]
fn q2_minimal_pdf_text_layer() {
    let pdf = minimal_pdf(&["Total: 1,234.56 EUR", "Due date: 15/10/2026"]);
    let text = extract_text_from_mem(&pdf);
    println!("Q2 exact: {text:?}");

    // Deliberately wrong xref *entry* offset (object 1's recorded byte offset, off by
    // a few bytes) while startxref itself still points at a well-formed xref table.
    let mut wrong = pdf.clone();
    let xref_pos = pdf
        .windows(b"xref\n".len())
        .position(|w| w == b"xref\n")
        .expect("xref keyword present");
    let after_keyword = xref_pos + b"xref\n".len();
    let header_line_end = pdf[after_keyword..]
        .iter()
        .position(|&b| b == b'\n')
        .map(|p| after_keyword + p + 1)
        .expect("xref header line newline");
    // Skip the free-list entry (object 0, 20 bytes) to reach object 1's offset digits.
    let entry1_start = header_line_end + 20;
    let digits = std::str::from_utf8(&wrong[entry1_start..entry1_start + 10])
        .expect("ascii digits")
        .parse::<u32>()
        .expect("numeric offset");
    let fixed = format!("{:010}", digits + 3);
    wrong[entry1_start..entry1_start + 10].copy_from_slice(fixed.as_bytes());
    let wrong_text = extract_text_from_mem(&wrong);
    println!("Q2 xref entry offset wrong by a few bytes: {wrong_text:?}");

    // Raw WinAnsi byte 0x80 (Euro sign) embedded directly in the PDF string literal;
    // this cannot be expressed as a Rust `&str` since it is not valid UTF-8 alone.
    let mut euro_content = b"BT /F1 12 Tf 72 720 Td (Price: ".to_vec();
    euro_content.push(0x80);
    euro_content.extend_from_slice(b"100) Tj ET");
    let euro_pdf = build_pdf(&euro_content, false);
    println!("Q2 euro byte 0x80 in string: {:?}", extract_text_from_mem(&euro_pdf));
}

#[test]
fn q3_no_text_layer_variants() {
    let empty_content = build_pdf(b"", false);
    println!("Q3 i empty content: {:?}", extract_text_from_mem(&empty_content));

    let rect_only = build_pdf(b"0 0 100 100 re f", false);
    println!("Q3 ii rectangle only: {:?}", extract_text_from_mem(&rect_only));

    let image = image_only_pdf();
    println!("Q3 iii image xobject: {:?}", extract_text_from_mem(&image));
}
