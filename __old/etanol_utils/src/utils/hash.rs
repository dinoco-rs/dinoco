use data_encoding::HEXUPPER;

pub fn hash(content: String) -> String {
    HEXUPPER.encode(content.as_bytes())
}

pub fn decode(content: String) -> String {
    String::from_utf8(HEXUPPER.decode(content.as_bytes()).unwrap()).unwrap()
}
