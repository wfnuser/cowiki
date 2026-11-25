//! Hash the actual system prompt and response_format bytes for the run manifest.
#[cfg(feature = "remote-verifier")]
fn main() {
    use cowiki_knowledge::governance::{
        digest,
        remote::{request_contract, Task},
    };
    for (name, task) in [
        ("verifier", Task::Verify),
        ("single_pass", Task::SinglePass),
    ] {
        let (prompt, format) = request_contract(task).expect("valid verifier schema");
        println!(
            "{} {} {}",
            name,
            digest(&prompt),
            digest(&format.to_string())
        );
    }
}
#[cfg(not(feature = "remote-verifier"))]
fn main() {
    eprintln!("Enable the remote-verifier feature to hash the wire contract.");
}
