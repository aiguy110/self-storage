use std::io::Read;

fn main() {
    selfstorage::self_storage_init();

    let mut previous_text = String::new();
    let mut reader = selfstorage::get_stored_data().unwrap();
    reader.read_to_string(&mut previous_text).unwrap();
    println!("Last text entered was: {}", previous_text);

    println!("Please enter some text, then hit enter.");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
    selfstorage::set_stored_data_and_exit(input.as_bytes());
}