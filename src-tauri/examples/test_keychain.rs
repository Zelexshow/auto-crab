fn main() {
    for key in ["deepseek", "dashscope", "dashscope_vl", "feishu-secret"] {
        match keyring::Entry::new("auto-crab", key) {
            Ok(entry) => match entry.get_password() {
                Ok(pw) => println!("{}: OK ({}****{})", key, &pw[..4.min(pw.len())], &pw[pw.len().saturating_sub(4)..]),
                Err(e) => println!("{}: MISSING - {}", key, e),
            },
            Err(e) => println!("{}: ERROR - {}", key, e),
        }
    }
}
