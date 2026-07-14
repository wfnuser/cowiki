fn main() {
    match cowiki_desktop_lib::run_mcp_if_requested() {
        Ok(true) => return,
        Ok(false) => {}
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
    cowiki_desktop_lib::run();
}
