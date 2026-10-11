#![cfg(feature = "nbt")]

use std::collections::HashMap;

use nbtx::{
    DecodeOptions, Error, Value, from_le_bytes, from_le_bytes_with, from_varint_bytes_with,
};

fn options(max_bytes: usize, max_nodes: usize, max_depth: usize) -> DecodeOptions {
    DecodeOptions {
        max_bytes,
        max_nodes,
        max_depth,
    }
}

fn list_of_empty_compounds(count: u32) -> Vec<u8> {
    let mut bytes = vec![9, 0, 0, 10];
    bytes.extend(count.to_le_bytes());
    bytes.extend(std::iter::repeat_n(0u8, count as usize));
    bytes
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

#[test]
fn input_past_max_bytes_is_refused() {
    let bytes = list_of_empty_compounds(100);
    let result = from_le_bytes_with::<Value>(&mut bytes.as_slice(), options(50, usize::MAX, 64));
    assert!(
        matches!(result, Err(Error::InputSizeExceeded { max: 50 })),
        "{result:?}"
    );
}

#[test]
fn values_past_max_nodes_are_refused() {
    let bytes = list_of_empty_compounds(1000);
    let as_value = from_le_bytes_with::<Value>(&mut bytes.as_slice(), options(usize::MAX, 100, 64));
    assert!(
        matches!(as_value, Err(Error::NodeCountExceeded { max: 100 })),
        "{as_value:?}"
    );

    let as_maps = from_le_bytes_with::<Vec<HashMap<String, i8>>>(
        &mut bytes.as_slice(),
        options(usize::MAX, 100, 64),
    );
    assert!(
        matches!(as_maps, Err(Error::NodeCountExceeded { max: 100 })),
        "{as_maps:?}"
    );
}

#[test]
fn nesting_past_max_depth_is_refused() {
    let limits = options(usize::MAX, usize::MAX, 8);
    assert!(from_le_bytes_with::<Value>(&mut nested_lists(8).as_slice(), limits).is_ok());
    assert!(matches!(
        from_le_bytes_with::<Value>(&mut nested_lists(9).as_slice(), limits),
        Err(Error::MaxDepthExceeded(_))
    ));
}

#[test]
fn a_document_within_its_limits_decodes() {
    let bytes = list_of_empty_compounds(10);
    let value = from_le_bytes_with::<Value>(&mut bytes.as_slice(), options(bytes.len(), 11, 2));
    assert!(value.is_ok(), "{value:?}");
}

#[test]
fn the_plain_functions_use_bounded_defaults() {
    let defaults = DecodeOptions::default();
    assert!(defaults.max_bytes < usize::MAX && defaults.max_nodes < usize::MAX);

    let count = defaults.max_nodes as u32;
    let bytes = list_of_empty_compounds(count);
    assert!(matches!(
        from_le_bytes::<Value>(&mut bytes.as_slice()),
        Err(Error::NodeCountExceeded { .. })
    ));
}

#[test]
fn varint_decoding_honours_the_limits_too() {
    let mut bytes = vec![9u8, 0, 10, 40];
    bytes.extend([0u8; 20]);
    let result =
        from_varint_bytes_with::<Value>(&mut bytes.as_slice(), options(usize::MAX, 10, 64));
    assert!(
        matches!(result, Err(Error::NodeCountExceeded { max: 10 })),
        "{result:?}"
    );
}
