//! Data model for parsed PX4 `.msg` files.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scalar {
    Bool,
    Char,
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
}

impl Scalar {
    /// Byte size as PX4's C++ codegen sees it.
    pub fn size(self) -> usize {
        match self {
            Self::Bool | Self::Char | Self::I8 | Self::U8 => 1,
            Self::I16 | Self::U16 => 2,
            Self::I32 | Self::U32 | Self::F32 => 4,
            Self::I64 | Self::U64 | Self::F64 => 8,
        }
    }

    /// The Rust type name emitted for this scalar.
    pub fn rust_type(self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::Char => "u8", // PX4's `char` is an unsigned byte in serialization terms
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
            Self::F32 => "f32",
            Self::F64 => "f64",
        }
    }

    pub fn parse(token: &str) -> Option<Self> {
        Some(match token {
            "bool" => Self::Bool,
            "char" => Self::Char,
            "int8" => Self::I8,
            "int16" => Self::I16,
            "int32" => Self::I32,
            "int64" => Self::I64,
            "uint8" => Self::U8,
            "uint16" => Self::U16,
            "uint32" => Self::U32,
            "uint64" => Self::U64,
            "float32" => Self::F32,
            "float64" => Self::F64,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone)]
pub enum FieldType {
    /// `uint32 x`
    Scalar(Scalar),
    /// `uint8[3] clip_counter`
    ScalarArray(Scalar, usize),
    /// `PositionSetpoint current` — references another `.msg`
    Nested(String),
    /// `PositionSetpoint[3] waypoints`
    NestedArray(String, usize),
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub ty: FieldType,
}

#[derive(Debug, Clone)]
pub struct Constant {
    pub ty: Scalar,
    pub name: String,
    /// Preserved as written; we emit it verbatim on the Rust side so
    /// numeric literal suffixes and hex forms survive.
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct MsgDef {
    /// CamelCase, from the filename (`SensorGyro`).
    pub name: String,
    /// snake_case, derived from `name` (`sensor_gyro`).
    pub snake_name: String,
    pub fields: Vec<Field>,
    pub constants: Vec<Constant>,
    /// Topic names produced by this msg. Defaults to `[snake_name]`
    /// unless the file has a `# TOPICS a b c` directive.
    pub topics: Vec<String>,
}

/// Convert CamelCase → snake_case using PX4's convention
/// (insert `_` before each uppercase letter that follows a lowercase).
pub fn camel_to_snake(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase() {
            let needs_underscore = i > 0
                && (chars[i - 1].is_ascii_lowercase()
                    || (chars[i - 1].is_ascii_uppercase()
                        && i + 1 < chars.len()
                        && chars[i + 1].is_ascii_lowercase()));
            if needs_underscore {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

#[derive(Debug)]
pub enum ParseError {
    Io(std::io::Error),
    Syntax { line: usize, msg: String },
    UnresolvedNested(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io: {e}"),
            Self::Syntax { line, msg } => write!(f, "line {line}: {msg}"),
            Self::UnresolvedNested(name) => {
                write!(
                    f,
                    "unresolved nested type `{name}` (no matching .msg found)"
                )
            }
        }
    }
}

impl std::error::Error for ParseError {}

impl From<std::io::Error> for ParseError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camel_to_snake_matches_px4_convention() {
        assert_eq!(camel_to_snake("SensorGyro"), "sensor_gyro");
        assert_eq!(camel_to_snake("VehicleStatus"), "vehicle_status");
        assert_eq!(camel_to_snake("ActuatorOutputs"), "actuator_outputs");
        assert_eq!(
            camel_to_snake("PositionSetpointTriplet"),
            "position_setpoint_triplet"
        );
        // Mixed-case acronym edge case.
        assert_eq!(camel_to_snake("GpsInfo"), "gps_info");
    }
}
