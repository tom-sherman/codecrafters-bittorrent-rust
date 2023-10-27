use clap::{command, Parser, Subcommand};
use hashes::Hashes;
use peers::Peers;
use serde::{self, Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    Decode { value: String },
    Info { torrent: PathBuf },
    Peers { torrent: PathBuf },
}

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
    pub fn info_hash(&self) -> hashes::Hash {
        let mut hasher = Sha1::new();
        let encoded_info = serde_bencode::to_bytes(&self.info).unwrap();
        hasher.update(encoded_info);
        hasher.finalize().into()
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

#[derive(Debug)]
struct Client<'a> {
    peer_id: String,
    port: u16,
    uploaded: u64,
    downloaded: u64,
    left: u64,
    torrent: &'a Torrent,
}

impl<'a> Client<'a> {
    pub fn new(torrent: &'a Torrent) -> Self {
        Self {
            // TODO: generate a random peer id
            peer_id: "00112233445566778899".to_owned(),
            left: torrent.info.length,
            port: 6881,
            uploaded: 0,
            downloaded: 0,
            torrent: torrent,
        }
    }

    pub async fn get_peers(&self) -> anyhow::Result<Peers> {
        let client = reqwest::Client::new();

        let mut request = client.get(self.torrent.announce.value().clone()).build()?;

        let query = serde_urlencoded::to_string(&TrackerRequest {
            compact: 1,
            downloaded: self.downloaded as usize,
            left: self.left as usize,
            peer_id: self.peer_id.clone(),
            port: self.port,
            uploaded: self.uploaded as usize,
        })?;

        request.url_mut().set_query(Some(&format!(
            "info_hash={}&{}",
            urlencode(&self.torrent.info_hash()).as_str(),
            query,
        )));

        let body = client.execute(request).await?.bytes().await?;

        Ok(serde_bencode::from_bytes::<TrackerResponse>(body.as_ref())?.peers)
    }
}

#[derive(Debug, Clone, Serialize)]
struct TrackerRequest {
    peer_id: String,
    port: u16,
    uploaded: usize,
    downloaded: usize,
    left: usize,
    compact: u8,
}

#[derive(Debug, Clone, Deserialize)]
struct TrackerResponse {
    #[serde(rename = "interval")]
    _interval: u64,
    peers: Peers,
}

// Thanks to @jonhoo for this code
mod hashes {
    use serde::de::{self, Deserialize, Deserializer, Visitor};
    use serde::ser::{Serialize, Serializer};
    use std::fmt;

    pub type Hash = [u8; 20];

    #[derive(Debug, Clone)]
    pub struct Hashes(pub Vec<Hash>);
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

mod peers {
    use serde::de::{self, Deserialize, Deserializer, Visitor};
    use std::fmt;

    #[derive(Debug, Clone)]
    pub struct Peer {
        pub ip: String,
        pub port: u16,
    }

    impl Peer {
        pub fn to_string(&self) -> String {
            format!("{}:{}", self.ip, self.port)
        }
    }

    #[derive(Debug, Clone)]
    pub struct Peers(pub Vec<Peer>);
    struct PeersVisitor;
    impl<'de> Visitor<'de> for PeersVisitor {
        type Value = Peers;
        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a byte string whose length is a multiple of 6")
        }
        fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            if v.len() % 6 != 0 {
                return Err(E::custom(format!("length is {}", v.len())));
            }
            // TODO: use array_chunks when stable
            Ok(Peers(
                v.chunks_exact(6)
                    .map(|slice_6| slice_6.try_into().expect("guaranteed to be length 6"))
                    .map(|chunk: [u8; 6]| {
                        let ip = format!("{}.{}.{}.{}", chunk[0], chunk[1], chunk[2], chunk[3]);
                        let port = u16::from_be_bytes([chunk[4], chunk[5]]);
                        Peer { ip, port }
                    })
                    .collect(),
            ))
        }
    }
    impl<'de> Deserialize<'de> for Peers {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_bytes(PeersVisitor)
        }
    }
}

// Usage: your_bittorrent.sh decode "<encoded_value>"
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Decode { value } => {
            let decoded_value = decode_bencoded_value(&value);
            println!("{}", decoded_value.to_string());
        }
        Command::Info { torrent } => {
            let torrent_file = std::fs::read(torrent)?;
            let torrent: Torrent = serde_bencode::from_bytes(&torrent_file)?;
            println!("Tracker URL: {}", torrent.announce.value());
            println!("Length: {}", torrent.info.length);
            println!("Info Hash: {}", hex::encode(torrent.info_hash()));
            println!("Piece Length: {}", torrent.info.piece_length);
            println!("Piece Hashes:");
            for hash in torrent.info.pieces.0 {
                println!("{}", hex::encode(hash));
            }
        }
        Command::Peers { torrent } => {
            let torrent_file = std::fs::read(torrent)?;
            let torrent: Torrent = serde_bencode::from_bytes(&torrent_file)?;

            let client = Client::new(&torrent);
            for peer in client.get_peers().await?.0 {
                println!("{}", peer.to_string());
            }
        }
    }

    Ok(())
}

fn urlencode(t: &[u8; 20]) -> String {
    let mut encoded = String::with_capacity(3 * t.len());
    for &byte in t {
        encoded.push('%');
        encoded.push_str(&hex::encode(&[byte]));
    }
    encoded
}
