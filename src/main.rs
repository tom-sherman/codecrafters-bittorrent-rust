use std::env;

fn interperet_value(value: serde_bencode::value::Value) -> serde_json::Value {
    match value {
        serde_bencode::value::Value::Int(i) => serde_json::Value::Number(i.into()),
        serde_bencode::value::Value::Bytes(b) => {
            serde_json::Value::String(String::from_utf8(b).unwrap())
        }
        serde_bencode::value::Value::List(l) => {
            let mut v = Vec::new();
            for i in l {
                v.push(interperet_value(i));
            }
            serde_json::Value::Array(v)
        }
        serde_bencode::value::Value::Dict(d) => {
            let mut m = serde_json::Map::new();
            for (k, v) in d {
                m.insert(String::from_utf8(k).unwrap(), interperet_value(v));
            }
            serde_json::Value::Object(m)
        }
    }
}

fn decode_bencoded_value(input: &str) -> serde_json::Value {
    interperet_value(serde_bencode::from_str(&input).unwrap())
}

// Usage: your_bittorrent.sh decode "<encoded_value>"
fn main() {
    let args: Vec<String> = env::args().collect();
    let command = &args[1];

    if command == "decode" {
        let encoded_value = &args[2];
        let decoded_value = decode_bencoded_value(&encoded_value);
        println!("{}", decoded_value.to_string());
    } else {
        println!("unknown command: {}", args[1])
    }
}
