use std::{fs, io::Write, path::Path};

use crate::{Error, Result};

fn invalid() -> Error {
    Error::Recognition
}

fn varint(data: &[u8], position: &mut usize) -> Result<u64> {
    let mut value = 0;
    for shift in (0..70).step_by(7) {
        let byte = *data.get(*position).ok_or_else(invalid)?;
        *position += 1;
        if shift == 63 && byte > 1 {
            return Err(invalid());
        }
        value |= u64::from(byte & 127) << shift;
        if byte < 128 {
            return Ok(value);
        }
    }
    Err(invalid())
}

fn write_varint(mut value: u64, output: &mut Vec<u8>) {
    while value >= 128 {
        output.push((value as u8) | 128);
        value >>= 7;
    }
    output.push(value as u8);
}

fn bytes(number: u64, data: &[u8], output: &mut Vec<u8>) {
    write_varint((number << 3) | 2, output);
    write_varint(data.len() as u64, output);
    output.extend_from_slice(data);
}

struct Field<'a> {
    number: u64,
    wire: u64,
    raw: &'a [u8],
    value: &'a [u8],
}

fn fields(data: &[u8]) -> Result<Vec<Field<'_>>> {
    let mut position = 0;
    let mut result = Vec::new();
    while position < data.len() {
        let start = position;
        let tag = varint(data, &mut position)?;
        let wire = tag & 7;
        if tag >> 3 == 0 {
            return Err(invalid());
        }
        let length = match wire {
            0 => {
                let begin = position;
                varint(data, &mut position)?;
                position - begin
            }
            1 => 8,
            2 => usize::try_from(varint(data, &mut position)?).map_err(|_| invalid())?,
            5 => 4,
            _ => return Err(invalid()),
        };
        let value_start = if wire == 0 {
            position - length
        } else {
            position
        };
        if wire != 0 {
            position = position.checked_add(length).ok_or_else(invalid)?;
        }
        let value = data.get(value_start..position).ok_or_else(invalid)?;
        result.push(Field {
            number: tag >> 3,
            wire,
            raw: &data[start..position],
            value,
        });
    }
    Ok(result)
}

fn inline_tensor(data: &[u8], root: &Path) -> Result<Vec<u8>> {
    let parsed = fields(data)?;
    if !parsed.iter().any(|field| field.number == 13) {
        return Ok(data.to_vec());
    }
    let mut location = None;
    let mut offset = 0usize;
    let mut length = None;
    for field in parsed.iter().filter(|field| field.number == 13) {
        let entry = fields(field.value)?;
        let text = |number| -> Result<&str> {
            std::str::from_utf8(
                entry
                    .iter()
                    .find(|f| f.number == number)
                    .ok_or_else(invalid)?
                    .value,
            )
            .map_err(|_| invalid())
        };
        match text(1)? {
            "location" => location = Some(text(2)?.to_owned()),
            "offset" => offset = text(2)?.parse().map_err(|_| invalid())?,
            "length" => length = Some(text(2)?.parse::<usize>().map_err(|_| invalid())?),
            _ => return Err(invalid()),
        }
    }
    if location.as_deref() != Some("encoder.int8.onnx.data") {
        return Err(invalid());
    }
    use std::io::{Read, Seek, SeekFrom};
    let mut file = fs::File::open(root.join("encoder.int8.onnx.data")).map_err(|_| invalid())?;
    let size =
        usize::try_from(file.metadata().map_err(|_| invalid())?.len()).map_err(|_| invalid())?;
    let length = length.ok_or_else(invalid)?;
    if offset
        .checked_add(length)
        .filter(|end| *end <= size)
        .is_none()
    {
        return Err(invalid());
    }
    file.seek(SeekFrom::Start(offset as u64))
        .map_err(|_| invalid())?;
    let mut weights = vec![0; length];
    file.read_exact(&mut weights).map_err(|_| invalid())?;
    let mut output = Vec::new();
    for field in parsed {
        if !matches!(field.number, 9 | 13 | 14) {
            output.extend_from_slice(field.raw);
        }
    }
    bytes(9, &weights, &mut output);
    Ok(output)
}

fn inline_model(data: &[u8], root: &Path) -> Result<Vec<u8>> {
    rewrite(data, root, 0, 0)
}

// Model -> Graph -> Node -> Attribute -> Tensor; unknown fields remain byte-exact.
fn rewrite(data: &[u8], root: &Path, kind: u8, depth: usize) -> Result<Vec<u8>> {
    if depth > 64 {
        return Err(invalid());
    }
    let mut output = Vec::new();
    for field in fields(data)? {
        let child = match (kind, field.number) {
            (0, 7) | (3, 6 | 11) => Some(1),
            (1, 1) => Some(2),
            (2, 5) => Some(3),
            (1, 5) | (3, 5 | 10) | (4, 1 | 2) => Some(5),
            (1, 15) | (3, 22 | 23) => Some(4),
            _ => None,
        };
        if let Some(child) = child {
            if field.wire != 2 {
                return Err(invalid());
            }
            let replacement = if child == 5 {
                inline_tensor(field.value, root)?
            } else {
                rewrite(field.value, root, child, depth + 1)?
            };
            bytes(field.number, &replacement, &mut output);
        } else {
            output.extend_from_slice(field.raw);
        }
    }
    Ok(output)
}

pub(crate) fn prepare_encoder(root: &Path) -> Result<tempfile::TempPath> {
    let model = fs::read(root.join("encoder.int8.onnx")).map_err(|_| invalid())?;
    let inline = inline_model(&model, root)?;
    let mut file = tempfile::Builder::new()
        .prefix("qms-inline-")
        .suffix(".onnx")
        .tempfile_in(root)
        .map_err(|_| invalid())?;
    file.write_all(&inline).map_err(|_| invalid())?;
    file.flush().map_err(|_| invalid())?;
    Ok(file.into_temp_path())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embeds_external_weights_and_preserves_other_fields() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("encoder.int8.onnx.data"),
            [10, 20, 30, 40],
        )
        .unwrap();
        let mut tensor = vec![8, 2];
        for (key, value) in [
            ("location", "encoder.int8.onnx.data"),
            ("offset", "1"),
            ("length", "2"),
        ] {
            let mut entry = Vec::new();
            bytes(1, key.as_bytes(), &mut entry);
            bytes(2, value.as_bytes(), &mut entry);
            bytes(13, &entry, &mut tensor);
        }
        tensor.extend([112, 1]);
        let mut graph = Vec::new();
        bytes(5, &tensor, &mut graph);
        fn constant(tensor: &[u8], graph: &mut Vec<u8>) {
            let mut attribute = Vec::new();
            bytes(5, tensor, &mut attribute);
            let mut node = Vec::new();
            bytes(5, &attribute, &mut node);
            bytes(1, &node, graph);
        }
        constant(&tensor, &mut graph);
        let mut model = vec![8, 8];
        bytes(7, &graph, &mut model);
        let result = inline_model(&model, directory.path()).unwrap();
        let mut expected_tensor = vec![8, 2];
        bytes(9, &[20, 30], &mut expected_tensor);
        let mut expected_graph = Vec::new();
        bytes(5, &expected_tensor, &mut expected_graph);
        constant(&expected_tensor, &mut expected_graph);
        let mut expected = vec![8, 8];
        bytes(7, &expected_graph, &mut expected);
        assert_eq!(result, expected);
        fs::write(directory.path().join("encoder.int8.onnx"), &model).unwrap();
        let temporary = prepare_encoder(directory.path()).unwrap();
        let path = temporary.to_path_buf();
        assert_eq!(fs::read(&path).unwrap(), expected);
        drop(temporary);
        assert!(!path.exists());
        assert_eq!(
            fs::read(directory.path().join("encoder.int8.onnx")).unwrap(),
            model
        );
    }

    #[test]
    fn malformed_protobuf_is_rejected() {
        for input in [&[128][..], &[58, 100, 1], &[15]] {
            assert!(inline_model(input, Path::new(".")).is_err());
        }
    }

    #[test]
    fn external_paths_and_out_of_bounds_ranges_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("encoder.int8.onnx.data"), [0]).unwrap();
        for (location, offset, length) in [
            ("../outside", "0", "1"),
            ("encoder.int8.onnx.data", "1", "1"),
            ("encoder.int8.onnx.data", "0", "999999999999999999999999"),
        ] {
            let mut tensor = Vec::new();
            for (key, value) in [
                ("location", location),
                ("offset", offset),
                ("length", length),
            ] {
                let mut entry = Vec::new();
                bytes(1, key.as_bytes(), &mut entry);
                bytes(2, value.as_bytes(), &mut entry);
                bytes(13, &entry, &mut tensor);
            }
            assert!(inline_tensor(&tensor, directory.path()).is_err());
        }
    }
}
