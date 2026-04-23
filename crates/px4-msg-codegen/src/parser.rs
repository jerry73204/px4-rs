//! Line-based parser for PX4 `.msg` files.
//!
//! Grammar (per line, after stripping `#`-introduced trailing comments):
//!
//! ```text
//! line      := blank | comment | topics | const | field
//! comment   := "#" ...
//! topics    := "# TOPICS" ident+                (leading-"#" handled before strip)
//! const     := scalar_ty ident "=" value
//! field     := type ident
//! type      := scalar_ty | scalar_ty "[" N "]" | nested | nested "[" N "]"
//! nested    := CamelCaseIdent
//! ```

use std::path::Path;

use crate::model::{Constant, Field, FieldType, MsgDef, ParseError, Scalar, camel_to_snake};

/// Parse a single `.msg` file into a `MsgDef`. The file's stem
/// (e.g. `SensorGyro` for `SensorGyro.msg`) becomes `MsgDef::name`.
pub fn parse_file(path: &Path) -> Result<MsgDef, ParseError> {
    let text = std::fs::read_to_string(path)?;
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| ParseError::Syntax {
            line: 0,
            msg: format!("invalid path: {}", path.display()),
        })?
        .to_string();
    parse_str(&name, &text)
}

/// Parse from a source string. Used by tests; takes the CamelCase
/// name explicitly (normally derived from the file stem).
pub fn parse_str(name: &str, text: &str) -> Result<MsgDef, ParseError> {
    let mut fields = Vec::new();
    let mut constants = Vec::new();
    let mut topics: Option<Vec<String>> = None;

    for (lineno_zero, raw_line) in text.lines().enumerate() {
        let lineno = lineno_zero + 1;

        // `# TOPICS` — before comment stripping, because the line itself
        // begins with `#`.
        let trimmed = raw_line.trim();
        if let Some(rest) = trimmed.strip_prefix('#') {
            let rest = rest.trim();
            if let Some(list) = rest.strip_prefix("TOPICS") {
                let names: Vec<String> = list.split_whitespace().map(ToString::to_string).collect();
                if names.is_empty() {
                    return Err(ParseError::Syntax {
                        line: lineno,
                        msg: "`# TOPICS` directive with no names".into(),
                    });
                }
                topics = Some(names);
            }
            // Otherwise an ordinary comment line — skip.
            continue;
        }

        // Strip trailing comment.
        let code = match trimmed.find('#') {
            Some(idx) => trimmed[..idx].trim(),
            None => trimmed,
        };
        if code.is_empty() {
            continue;
        }

        // Constant: `type ident = value`
        if let Some(eq_idx) = code.find('=') {
            let lhs = code[..eq_idx].trim();
            let rhs = code[eq_idx + 1..].trim();
            let mut parts = lhs.split_whitespace();
            let ty_tok = parts.next().ok_or_else(|| ParseError::Syntax {
                line: lineno,
                msg: "missing type in constant".into(),
            })?;
            let name_tok = parts.next().ok_or_else(|| ParseError::Syntax {
                line: lineno,
                msg: "missing name in constant".into(),
            })?;
            if parts.next().is_some() {
                return Err(ParseError::Syntax {
                    line: lineno,
                    msg: "unexpected tokens before `=` in constant".into(),
                });
            }
            let ty = Scalar::parse(ty_tok).ok_or_else(|| ParseError::Syntax {
                line: lineno,
                msg: format!("constant type must be a scalar, got `{ty_tok}`"),
            })?;
            constants.push(Constant {
                ty,
                name: name_tok.to_string(),
                value: rhs.to_string(),
            });
            continue;
        }

        // Field: `type ident`
        let mut parts = code.split_whitespace();
        let ty_tok = parts.next().ok_or_else(|| ParseError::Syntax {
            line: lineno,
            msg: "empty line after comment strip".into(),
        })?;
        let name_tok = parts.next().ok_or_else(|| ParseError::Syntax {
            line: lineno,
            msg: "missing field name".into(),
        })?;
        if parts.next().is_some() {
            return Err(ParseError::Syntax {
                line: lineno,
                msg: "unexpected extra tokens in field line".into(),
            });
        }

        let ty = parse_field_type(ty_tok).ok_or_else(|| ParseError::Syntax {
            line: lineno,
            msg: format!("cannot parse field type `{ty_tok}`"),
        })?;
        fields.push(Field {
            name: name_tok.to_string(),
            ty,
        });
    }

    let snake_name = camel_to_snake(name);
    let topics = topics.unwrap_or_else(|| vec![snake_name.clone()]);

    Ok(MsgDef {
        name: name.to_string(),
        snake_name,
        fields,
        constants,
        topics,
    })
}

fn parse_field_type(tok: &str) -> Option<FieldType> {
    // Detect array suffix `[N]` at the end.
    if let Some(open) = tok.find('[') {
        if !tok.ends_with(']') {
            return None;
        }
        let base = &tok[..open];
        let n_str = &tok[open + 1..tok.len() - 1];
        let n: usize = n_str.parse().ok()?;
        if let Some(s) = Scalar::parse(base) {
            return Some(FieldType::ScalarArray(s, n));
        }
        if is_camel_case(base) {
            return Some(FieldType::NestedArray(base.to_string(), n));
        }
        return None;
    }

    if let Some(s) = Scalar::parse(tok) {
        return Some(FieldType::Scalar(s));
    }
    if is_camel_case(tok) {
        return Some(FieldType::Nested(tok.to_string()));
    }
    None
}

fn is_camel_case(s: &str) -> bool {
    match s.chars().next() {
        Some(c) if c.is_ascii_uppercase() => {
            s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sensor_gyro() {
        let text = "\
uint64 timestamp          # time since system start (microseconds)
uint64 timestamp_sample

uint32 device_id          # unique device ID

float32 x                 # angular velocity
float32 y
float32 z

float32 temperature

uint32 error_count

uint8[3] clip_counter

uint8 samples

uint8 ORB_QUEUE_LENGTH = 8
";
        let def = parse_str("SensorGyro", text).unwrap();
        assert_eq!(def.name, "SensorGyro");
        assert_eq!(def.snake_name, "sensor_gyro");
        assert_eq!(def.fields.len(), 10);
        assert_eq!(def.constants.len(), 1);
        assert_eq!(def.constants[0].name, "ORB_QUEUE_LENGTH");
        assert_eq!(def.topics, vec!["sensor_gyro"]);

        match &def.fields[8].ty {
            FieldType::ScalarArray(Scalar::U8, 3) => {}
            other => panic!("expected uint8[3], got {other:?}"),
        }
    }

    #[test]
    fn parses_topics_directive() {
        let text = "\
uint64 timestamp
float32[16] output

# TOPICS actuator_outputs actuator_outputs_sim actuator_outputs_debug
";
        let def = parse_str("ActuatorOutputs", text).unwrap();
        assert_eq!(
            def.topics,
            vec![
                "actuator_outputs",
                "actuator_outputs_sim",
                "actuator_outputs_debug",
            ]
        );
    }

    #[test]
    fn parses_nested_field() {
        let text = "\
uint64 timestamp
PositionSetpoint previous
PositionSetpoint current
PositionSetpoint next
";
        let def = parse_str("PositionSetpointTriplet", text).unwrap();
        assert_eq!(def.fields.len(), 4);
        match &def.fields[1].ty {
            FieldType::Nested(name) if name == "PositionSetpoint" => {}
            other => panic!("expected Nested(PositionSetpoint), got {other:?}"),
        }
    }
}
