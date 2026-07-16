
fn main() {
    let command = std::env::args().nth(1).unwrap_or_else(|| "help".to_owned());
    match command.as_str() {
        "status" => println!("Dennett skeleton: no running installation discovered"),
        "doctor" => println!("Run python tools/verify_repo.py for repository checks"),
        _ => println!("dennettctl skeleton commands: status, doctor"),
    }
}
