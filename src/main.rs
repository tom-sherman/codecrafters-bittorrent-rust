use serde_json;
use std::env;

// Available if you need it!
// use serde_bencode

fn decode_bencoded_value(encoded_value: &str) -> serde_json::Value {
    if let Some((length, str)) = encoded_value.split_once(":") {
        if let Ok(length) = length.parse::<usize>() {
            return serde_json::Value::String(str[..length].to_string());
        }
    } else if encoded_value.starts_with("i") && encoded_value.ends_with("e") {
        if let Ok(number) = encoded_value[1..encoded_value.len() - 1].parse::<i64>() {
            return serde_json::Value::Number(serde_json::Number::from(number));
        }
    }
    panic!("Unhandled encoded value: {}", encoded_value)
}

// Usage: your_bittorrent.sh decode "<encoded_value>"
fn main() {
    let args: Vec<String> = env::args().collect();
    let command = &args[1];

    if command == "decode" {
        let encoded_value = &args[2];
        let decoded_value = decode_bencoded_value(encoded_value);
        println!("{}", decoded_value.to_string());
    } else {
        println!("unknown command: {}", args[1])
    }
}
