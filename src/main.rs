use hashes::Hashes;
use serde::{self, Deserialize, Serialize};
use sha1::{Digest, Sha1};
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

#[derive(Debug)]
struct Url(reqwest::Url);

impl Url {
    pub fn value(&self) -> &reqwest::Url {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Url {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let str = String::deserialize(deserializer)?;
        Ok(Url(
            reqwest::Url::parse(&str).map_err(serde::de::Error::custom)?
        ))
    }
}

impl Serialize for Url {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.to_string().serialize(serializer)
    }
}

#[derive(Deserialize, Serialize, Debug)]
struct Torrent {
    /// URL to a "tracker", which is a central server that keeps track of peers participating in the sharing of a torrent.
    announce: Url,
    info: Info,
}

impl Torrent {
    pub fn info_hash(&self) -> String {
        let mut hasher = Sha1::new();
        let encoded_info = serde_bencode::to_bytes(&self.info).unwrap();
        hasher.update(encoded_info);
        let info_hash = hasher.finalize();
        hex::encode(&info_hash)
    }
}

#[derive(Deserialize, Serialize, Debug)]
struct Info {
    /// size of the file in bytes, for single-file torrents
    length: u64,
    /// suggested name to save the file / directory as
    name: String,
    /// number of bytes in each piece
    #[serde(rename = "piece length")]
    piece_length: u64,
    /// SHA-1 hashes of each piece
    pieces: Hashes,
}

// Thanks to @jonhoo for this code
mod hashes {
    use serde::de::{self, Deserialize, Deserializer, Visitor};
    use serde::ser::{Serialize, Serializer};
    use std::fmt;
    #[derive(Debug, Clone)]
    pub struct Hashes(pub Vec<[u8; 20]>);
    struct HashesVisitor;
    impl<'de> Visitor<'de> for HashesVisitor {
        type Value = Hashes;
        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a byte string whose length is a multiple of 20")
        }
        fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            if v.len() % 20 != 0 {
                return Err(E::custom(format!("length is {}", v.len())));
            }
            // TODO: use array_chunks when stable
            Ok(Hashes(
                v.chunks_exact(20)
                    .map(|slice_20| slice_20.try_into().expect("guaranteed to be length 20"))
                    .collect(),
            ))
        }
    }
    impl<'de> Deserialize<'de> for Hashes {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_bytes(HashesVisitor)
        }
    }

    impl Serialize for Hashes {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let single_slice = self.0.concat();
            serializer.serialize_bytes(&single_slice)
        }
    }
}

// Usage: your_bittorrent.sh decode "<encoded_value>"
fn main() {
    let args: Vec<String> = env::args().collect();
    let command = &args[1];

    if command == "decode" {
        let encoded_value = &args[2];
        let decoded_value = decode_bencoded_value(&encoded_value);
        println!("{}", decoded_value.to_string());
    } else if command == "info" {
        let torrent_file_name = &args[2];
        // Read file as string
        let torrent_file = std::fs::read(torrent_file_name).unwrap();
        let torrent: Torrent = serde_bencode::from_bytes(&torrent_file).unwrap();

        println!("Tracker URL: {}", torrent.announce.value());
        println!("Length: {}", torrent.info.length);
        println!("Info Hash: {}", torrent.info_hash());
    } else {
        println!("unknown command: {}", args[1])
    }
}
