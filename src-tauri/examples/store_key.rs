fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: store_key <name> <secret>");
        std::process::exit(1);
    }
    let entry = keyring::Entry::new("auto-crab", &args[1]).expect("keyring entry");
    entry.set_password(&args[2]).expect("set password");
    println!("Stored '{}' in system keychain", args[1]);
}
