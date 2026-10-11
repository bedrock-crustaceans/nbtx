#![cfg(feature = "nbt")]

use std::collections::HashMap;
use std::time::{Duration, Instant};

fn compound_of_unique_bytes(count: usize) -> Vec<u8> {
    let mut bytes = vec![10, 0, 0];
    for i in 0..count {
        let key = format!("k{i}");
        bytes.push(1);
        bytes.extend((key.len() as u16).to_le_bytes());
        bytes.extend(key.as_bytes());
        bytes.push(0);
    }
    bytes.push(0);
    bytes
}

#[test]
fn decoding_many_unique_keys_into_a_map_is_not_quadratic() {
    let count = 60_000;
    let bytes = compound_of_unique_bytes(count);

    let started = Instant::now();
    let map: HashMap<String, i8> = nbtx::from_le_bytes(&mut bytes.as_slice()).unwrap();
    let elapsed = started.elapsed();

    assert_eq!(map.len(), count);
    assert!(
        elapsed < Duration::from_secs(3),
        "{count} unique keys took {elapsed:?}"
    );
}
