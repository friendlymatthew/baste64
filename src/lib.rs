#![warn(clippy::nursery)]
#![cfg(target_arch = "wasm32")]

use std::arch::wasm32::v128;
use std::slice;

use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::wasm_bindgen;

use decode_chunk::{decode_chunk, decoded_len};
use encode_chunk::{encode_chunk, encoded_len};

mod base64;
mod decode_chunk;
mod encode_chunk;
mod fuzz;
pub mod impl_v128;

/// [`atob`] decodes a string of data from an ascii string.
#[wasm_bindgen]
pub fn atob(ascii: String) -> Result<String, JsValue> {
    Ok(unsafe { String::from_utf8_unchecked(decode(ascii.as_bytes())?) })
}

/// [`btoa`] encodes a string of data to an ascii string.
#[wasm_bindgen]
pub fn btoa(binary_string: String) -> Result<String, JsValue> {
    encode_to_utf8(binary_string.as_bytes())
}

/// [`encode`] converts bytes into a base64-encoded byte array.
#[wasm_bindgen]
pub fn encode(data: &[u8]) -> Result<Vec<u8>, JsValue> {
    let mut ascii = Vec::new();
    encode_to(data, &mut ascii).map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(ascii)
}

#[wasm_bindgen]
pub fn encode_to_utf8(data: &[u8]) -> Result<String, JsValue> {
    Ok(unsafe { String::from_utf8_unchecked(encode(data)?) })
}

/// [`decode`] takes ascii and returns its original binary representation.
#[wasm_bindgen]
pub fn decode(ascii: &[u8]) -> Result<Vec<u8>, JsValue> {
    let mut data = Vec::new();
    decode_to(ascii, &mut data)?;
    Ok(data)
}

fn encode_to(data: &[u8], out: &mut Vec<u8>) -> Result<(), String> {
    if data.is_empty() {
        return Err(String::from("empty data"));
    }

    out.reserve(encoded_len(data.len()) + 16);
    let mut raw_out = out.as_mut_ptr_range().end;

    let mut start = data.as_ptr();
    let end = unsafe {
        if data.len() % 12 >= 4 {
            start.add(data.len() - data.len() % 12)
        } else if data.len() < 16 {
            start
        } else {
            start.add(data.len() - data.len() % 12 - 12)
        }
    };

    while start != end {
        let chunk = unsafe { slice::from_raw_parts(start, 16) };
        let chunk: &[u8; 16] = chunk.try_into().expect("Slice with incorrect length");
        let encoded = encode_chunk(chunk);

        unsafe {
            start = start.add(12);

            raw_out.cast::<v128>().write_unaligned(encoded);
            raw_out = raw_out.add(16);
        }
    }

    let end = data.as_ptr_range().end;
    while start < end {
        let chunk = unsafe {
            let rest = end.offset_from(start) as usize;
            slice::from_raw_parts(start, rest.min(12))
        };

        let mut temp_chunk = [0u8; 16];
        temp_chunk[0..chunk.len()].copy_from_slice(chunk);

        let encoded = encode_chunk(&temp_chunk);

        unsafe {
            start = start.add(chunk.len());

            raw_out.cast::<v128>().write_unaligned(encoded);
            raw_out = raw_out.add(encoded_len(chunk.len()));
        }
    }

    unsafe {
        let new_len = raw_out.offset_from(out.as_ptr());
        out.set_len(new_len as usize);
    }

    match out.len() % 4 {
        2 => out.extend_from_slice(b"=="),
        3 => out.extend_from_slice(b"="),
        _ => {}
    }

    Ok(())
}

pub fn decode_to(data: &[u8], out: &mut Vec<u8>) -> Result<(), String> {
    let data = match data {
        [p @ .., b'=', b'='] | [p @ .., b'='] | p => p,
    };

    if data.is_empty() {
        return Ok(());
    }

    out.reserve(decoded_len(data.len()) + 16);
    let mut raw_out = out.as_mut_ptr_range().end;

    let mut chunks = data.chunks_exact(16);
    let mut failed = false;

    for chunk in &mut chunks {
        let ascii = chunk.try_into().expect("Slice with incorrect length");
        let decoded = decode_chunk(ascii);
        failed |= decoded.is_err();
        let decoded = decoded.unwrap();

        unsafe {
            raw_out.cast::<v128>().write_unaligned(decoded);
            raw_out = raw_out.add(12);
        }
    }

    let rest = chunks.remainder();
    if !rest.is_empty() {
        let mut ascii = [b'A'; 16];
        ascii[0..rest.len()].copy_from_slice(rest);
        let decoded = decode_chunk(&ascii);
        failed |= decoded.is_err();
        let decoded = decoded.unwrap();

        unsafe {
            raw_out.cast::<v128>().write_unaligned(decoded);
            raw_out = raw_out.add(decoded_len(rest.len()));
        }
    }

    if failed {
        return Err(String::from("the decoding process failed unexpectedly"));
    }

    unsafe {
        let new_len = raw_out.offset_from(out.as_ptr());
        out.set_len(new_len as usize);
    }

    Ok(())
}
#[cfg(test)]
mod tests {
    use wasm_bindgen_test::wasm_bindgen_test;

    use super::*;

    #[wasm_bindgen_test]
    fn test_hello_world() -> Result<(), JsValue> {
        let encoded_data = b"SGVsbG8gV29ybGQ=";
        let raw_data = b"Hello World";

        let mut out = Vec::new();
        decode_to(encoded_data, &mut out)?;
        assert_eq!(out, raw_data);

        out = Vec::new();
        encode_to(raw_data, &mut out)?;
        assert_eq!(out, encoded_data);
        Ok(())
    }

    #[wasm_bindgen_test]
    fn test_readme_example() -> Result<(), JsValue> {
        let ascii = b"VGhlIGRvZyBsaWNrZWQgdGhlIG9pbCwgYW5kIGV2ZXJ5Ym9keSBsYXVnaGVkLg==";
        let message = decode(ascii)?;
        // The dog licked the oil, and everybody laughed.

        let encoded_to_ascii = encode(&message)?;
        assert_eq!(encoded_to_ascii, ascii.to_vec());

        Ok(())
    }
}
