pub fn main() {
    let mut args = std::env::args();
    let _ = args.next().unwrap(); // skip argv[0]
    match (args.next(), args.next()) {
        (Some(x), None) if x == "--branding" => println!("{}", chronahle_version::branding()),
        (None, _) => println!("{}", chronahle_version::VERSION),
        _ => panic!(),
    }
}
