use colored::*;
use comfy_table::Table;

/// Print a comfy-table to stdout.
pub fn print_table(mut table: Table) {
    table.load_preset(comfy_table::presets::UTF8_FULL);
    println!("{table}");
}

/// Print a success message (green).
pub fn print_success(msg: &str) {
    println!("{} {msg}", "✓".green().bold());
}

/// Print an error message (red).
pub fn print_error(msg: &str) {
    eprintln!("{} {msg}", "✗".red().bold());
}

/// Print an informational message (cyan).
pub fn print_info(msg: &str) {
    println!("{} {msg}", "ℹ".cyan());
}
