# Extraction fixtures

`mixed-text-scan.pdf` is a synthetic two-page PDF. Page 1 has the text layer
`DIGITAL PAGE EVIDENCE`; page 2 has only a raster image of `SCANNED PAGE EVIDENCE`.
It contains no user documents or external source material.

Regenerate with `python3 generate_mixed_pdf.py` (Poppler `pdftoppm` required).
Normal Rust tests verify that native extraction cannot import only page 1.
To test actual recovery with Poppler and Tesseract installed, run:

```sh
cargo test --locked --manifest-path web/src-tauri/Cargo.toml mixed_pdf_recovers_scanned_page_with_local_tools -- --ignored --nocapture
```
