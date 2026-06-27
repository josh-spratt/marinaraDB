use crate::{ColType, Scottatura, Value};

pub fn is_fixed_width(col_type: &ColType) -> bool {
    matches!(col_type, ColType::Integer | ColType::Real)
}

pub fn encoded_fixed_size(col_type: &ColType) -> usize {
    match col_type {
        ColType::Integer | ColType::Real => 8,
        ColType::Text | ColType::Blob => 0,
    }
}

pub fn encode_value(val: &Value, col_type: &ColType) -> Result<Vec<u8>, Scottatura> {
    match (val, col_type) {
        (Value::Null, ColType::Integer | ColType::Real) => Ok(vec![0u8; 8]),
        (Value::Null, ColType::Text | ColType::Blob) => {
            Ok(0xFFFF_FFFFu32.to_le_bytes().to_vec())
        }
        (Value::Integer(n), ColType::Integer) => Ok(n.to_le_bytes().to_vec()),
        (Value::Integer(n), ColType::Real) => Ok((*n as f64).to_le_bytes().to_vec()),
        (Value::Real(f), ColType::Real) => Ok(f.to_le_bytes().to_vec()),
        (Value::Text(s), ColType::Text) => {
            let b = s.as_bytes();
            let mut out = (b.len() as u32).to_le_bytes().to_vec();
            out.extend_from_slice(b);
            Ok(out)
        }
        (Value::Blob(b), ColType::Blob) => {
            let mut out = (b.len() as u32).to_le_bytes().to_vec();
            out.extend_from_slice(b);
            Ok(out)
        }
        (got, expected) => Err(Scottatura::TypeMismatch {
            got: format!("{got:?}"),
            col: String::new(),
            expected: format!("{expected:?}"),
        }),
    }
}

pub fn decode_value(bytes: &[u8], col_type: &ColType, is_null: bool) -> Value {
    if is_null {
        return Value::Null;
    }
    match col_type {
        ColType::Integer => Value::Integer(i64::from_le_bytes(bytes[..8].try_into().unwrap())),
        ColType::Real => Value::Real(f64::from_le_bytes(bytes[..8].try_into().unwrap())),
        ColType::Text => {
            let len = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
            if len == 0xFFFF_FFFF {
                return Value::Null;
            }
            Value::Text(String::from_utf8(bytes[4..4 + len].to_vec()).unwrap_or_default())
        }
        ColType::Blob => {
            let len = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
            if len == 0xFFFF_FFFF {
                return Value::Null;
            }
            Value::Blob(bytes[4..4 + len].to_vec())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_integer() {
        let val = Value::Integer(42);
        let encoded = encode_value(&val, &ColType::Integer).unwrap();
        assert_eq!(encoded.len(), 8);
        let decoded = decode_value(&encoded, &ColType::Integer, false);
        assert_eq!(decoded, Value::Integer(42));
    }

    #[test]
    fn test_encode_decode_negative_integer() {
        let val = Value::Integer(-999);
        let encoded = encode_value(&val, &ColType::Integer).unwrap();
        let decoded = decode_value(&encoded, &ColType::Integer, false);
        assert_eq!(decoded, Value::Integer(-999));
    }

    #[test]
    fn test_encode_decode_real() {
        let val = Value::Real(3.14);
        let encoded = encode_value(&val, &ColType::Real).unwrap();
        assert_eq!(encoded.len(), 8);
        let decoded = decode_value(&encoded, &ColType::Real, false);
        assert_eq!(decoded, Value::Real(3.14));
    }

    #[test]
    fn test_encode_decode_text() {
        let val = Value::Text("hello".to_string());
        let encoded = encode_value(&val, &ColType::Text).unwrap();
        assert_eq!(encoded.len(), 4 + 5);
        let decoded = decode_value(&encoded, &ColType::Text, false);
        assert_eq!(decoded, Value::Text("hello".to_string()));
    }

    #[test]
    fn test_encode_decode_empty_text() {
        let val = Value::Text(String::new());
        let encoded = encode_value(&val, &ColType::Text).unwrap();
        assert_eq!(encoded.len(), 4);
        let decoded = decode_value(&encoded, &ColType::Text, false);
        assert_eq!(decoded, Value::Text(String::new()));
    }

    #[test]
    fn test_encode_decode_blob() {
        let val = Value::Blob(vec![0xde, 0xad, 0xbe, 0xef]);
        let encoded = encode_value(&val, &ColType::Blob).unwrap();
        assert_eq!(encoded.len(), 4 + 4);
        let decoded = decode_value(&encoded, &ColType::Blob, false);
        assert_eq!(decoded, Value::Blob(vec![0xde, 0xad, 0xbe, 0xef]));
    }

    #[test]
    fn test_null_returns_null_regardless_of_bytes() {
        let bytes = 42i64.to_le_bytes().to_vec();
        let decoded = decode_value(&bytes, &ColType::Integer, true);
        assert_eq!(decoded, Value::Null);
    }

    #[test]
    fn test_integer_promoted_to_real() {
        let val = Value::Integer(5);
        let encoded = encode_value(&val, &ColType::Real).unwrap();
        let decoded = decode_value(&encoded, &ColType::Real, false);
        assert_eq!(decoded, Value::Real(5.0));
    }

    #[test]
    fn test_type_mismatch_errors() {
        let val = Value::Text("hello".to_string());
        let result = encode_value(&val, &ColType::Integer);
        assert!(matches!(result, Err(Scottatura::TypeMismatch { .. })));
    }

    #[test]
    fn test_fixed_width_classification() {
        assert!(is_fixed_width(&ColType::Integer));
        assert!(is_fixed_width(&ColType::Real));
        assert!(!is_fixed_width(&ColType::Text));
        assert!(!is_fixed_width(&ColType::Blob));
    }
}
