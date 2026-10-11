#![cfg(all(feature = "nbt", feature = "snbt"))]

use nbtx::{MAX_DEPTH, Value, from_le_bytes, to_le_bytes};

const WINDOWS_MAIN_THREAD_STACK: usize = 1 << 20;

fn on_small_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(WINDOWS_MAIN_THREAD_STACK)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap();
}

fn nested_lists(depth: usize) -> Vec<u8> {
    let mut bytes = vec![9, 0, 0];
    for _ in 1..depth {
        bytes.push(9);
        bytes.extend(1u32.to_le_bytes());
    }
    bytes.push(1);
    bytes.extend(0u32.to_le_bytes());
    bytes
}

fn nested_compounds(depth: usize) -> Vec<u8> {
    let mut bytes = vec![10, 0, 0];
    for _ in 1..depth {
        bytes.extend([10, 1, 0, b'c']);
    }
    bytes.extend(std::iter::repeat_n(0u8, depth));
    bytes
}

#[test]
fn a_document_at_max_depth_decodes_and_encodes_on_a_one_mib_stack() {
    on_small_stack(|| {
        let lists: Value = from_le_bytes(&mut nested_lists(MAX_DEPTH).as_slice()).unwrap();
        let compounds: Value = from_le_bytes(&mut nested_compounds(MAX_DEPTH).as_slice()).unwrap();
        to_le_bytes(&lists).unwrap();
        to_le_bytes(&compounds).unwrap();
    });
}

#[test]
fn snbt_at_max_depth_parses_on_a_one_mib_stack() {
    on_small_stack(|| {
        let text = format!("{}{}", "[".repeat(MAX_DEPTH), "]".repeat(MAX_DEPTH));
        nbtx::from_string::<Value>(&text).unwrap();
    });
}
