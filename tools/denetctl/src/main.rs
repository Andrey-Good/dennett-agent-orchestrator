
fn main() {
    let command = std::env::args().nth(1).unwrap_or_else(|| "help".to_owned());
    match command.as_str() {
        "status" => println!("Denet skeleton: no running installation discovered"),
        "doctor" => println!("Run python tools/verify_repo.py for repository checks"),
        _ => println!("denetctl skeleton commands: status, doctor"),
    }
}
