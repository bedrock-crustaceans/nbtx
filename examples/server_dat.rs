//! The servers.dat is used to store information regarding the multiplayer servers that the player
//! has added to their server list. It does not store the direct connect IP address (see options.txt),
//! nor any LAN server information. It is stored as an uncompressed NBT file. It is possible to add
//! color codes by editing the server name attribute with an NBT Editor through the use of formatting
//! codes.
//!
//! The file is located in the root of the directory specified in the launcher profile. By default,
//! this would be `.minecraft\servers.dat`.

use byteorder::BigEndian;
use serde::{Deserialize, Serialize};
use std::io::Cursor;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename = "")]
struct ServerDat {
    servers: Vec<ServerDatItem>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct ServerDatItem {
    icon: Option<String>,
    ip: String,
    name: String,
    accept_textures: Option<bool>,
}

fn main() {
    let servers_dat = include_bytes!("servers.dat");
    let mut servers_dat = Cursor::new(servers_dat.as_slice());

    let server_dat: ServerDat = nbtx::from_bytes::<BigEndian, _>(&mut servers_dat).unwrap();

    println!("{:#?}", server_dat);
}
