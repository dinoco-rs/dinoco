use ansi_term::Colour;

pub struct EtanolError {}

impl EtanolError {
    fn block(msg: String) -> String {
        let white = Colour::White.bold();
        let red = Colour::Red.bold();

        format!("{}{}{}", white.paint("["), red.paint(msg), white.paint("]"))
    }

    pub fn new(reason: String, errorType: String) {
        let white = Colour::White.bold();
        let block = EtanolError::block("EtanolError".to_string());

        println!("{} {}", block, white.paint(errorType));
        println!("{}", reason);

        std::process::exit(1);
    }
}
