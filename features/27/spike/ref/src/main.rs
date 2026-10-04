mod inflate;
mod xml;
mod zip;

use std::env;
use std::fs;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("unzip") => {
            let (zip_path, out_dir) = (&args[2], &args[3]);
            let data = fs::read(zip_path).expect("read zip");
            let entries = zip::read_entries(&data).expect("read entries");
            for (name, bytes) in entries {
                let safe_name = name.replace('/', "_");
                fs::write(format!("{out_dir}/{safe_name}"), bytes).expect("write entry");
            }
            ExitCode::SUCCESS
        }
        Some("inflate") => {
            let (in_path, out_path) = (&args[2], &args[3]);
            let data = fs::read(in_path).expect("read input");
            match inflate::inflate(&data) {
                Ok(out) => {
                    fs::write(out_path, out).expect("write output");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("inflate error: {e:?}");
                    ExitCode::FAILURE
                }
            }
        }
        Some("pluck") => {
            // pluck <file.xml> — demonstrates the plucker on the embedded
            // AttachedDocument -> Description -> CDATA Invoice structure.
            let path = &args[2];
            let text = fs::read_to_string(path).expect("read xml");
            let desc = xml::find_text(&text, &["Attachment", "ExternalReference"], "Description")
                .expect("Description not found");
            let payable = xml::find_text(&desc, &["LegalMonetaryTotal"], "PayableAmount");
            let currency = xml::find_attr(&desc, "PayableAmount", "currencyID");
            println!("PayableAmount present: {}", payable.is_some());
            println!("currencyID: {currency:?}");
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("usage: spike27 <unzip|inflate|pluck> ...");
            ExitCode::FAILURE
        }
    }
}
