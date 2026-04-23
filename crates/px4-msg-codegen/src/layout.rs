//! PX4-compatible struct layout: stable-sort fields by size descending,
//! insert padding before each nested type to 8-byte alignment, pad tail
//! to 8-byte alignment.
//!
//! This mirrors `Tools/msg/px_generate_uorb_topic_helper.py::add_padding_bytes`
//! in the PX4 source tree (v1.16.2). Keeping byte-for-byte parity with
//! the C++ output is phase 05's whole job.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::model::{Constant, Field, FieldType, MsgDef, ParseError};
use crate::parser;

const ALIGN_TO: usize = 8;

#[derive(Debug, Clone)]
pub enum LaidOutField {
    /// Field from the source .msg.
    Real(Field),
    /// `_padding<N>: [u8; SIZE]` inserted by the layout rules.
    Padding { index: usize, size: usize },
}

#[derive(Debug, Clone)]
pub struct LaidOutMsg {
    pub name: String,
    pub snake_name: String,
    pub fields: Vec<LaidOutField>,
    pub constants: Vec<Constant>,
    pub topics: Vec<String>,
    pub size: usize,
}

/// Context for resolving nested types. Caches parsed + laid-out
/// dependencies so each .msg is processed at most once.
pub struct Resolver {
    search: Vec<PathBuf>,
    cache: HashMap<String, LaidOutMsg>,
}

impl Resolver {
    pub fn new(search: Vec<PathBuf>) -> Self {
        Self {
            search,
            cache: HashMap::new(),
        }
    }

    /// Lay out a `.msg` file by name (e.g. `"PositionSetpoint"`).
    pub fn layout_by_name(&mut self, name: &str) -> Result<LaidOutMsg, ParseError> {
        if let Some(cached) = self.cache.get(name) {
            return Ok(cached.clone());
        }
        let path = self.find(name)?;
        let def = parser::parse_file(&path)?;
        self.layout(&def)
    }

    /// Lay out a parsed `MsgDef`. Nested types are resolved via the
    /// configured search path.
    pub fn layout(&mut self, def: &MsgDef) -> Result<LaidOutMsg, ParseError> {
        let mut sized: Vec<(usize, Field)> = Vec::with_capacity(def.fields.len());
        for f in &def.fields {
            let sz = self.field_sort_size(&f.ty)?;
            sized.push((sz, f.clone()));
        }
        // Stable sort by size descending. PX4 puts non-builtin at the
        // end by assigning them sort-size 0.
        sized.sort_by(|a, b| b.0.cmp(&a.0));

        let mut out = Vec::with_capacity(sized.len() + 4);
        let mut size_accum = 0usize;
        let mut padding_idx = 0usize;

        for (_sort_size, field) in sized {
            let field_size = self.field_total_size(&field.ty)?;

            // Padding before a nested type to 8-byte alignment.
            if matches!(
                field.ty,
                FieldType::Nested(_) | FieldType::NestedArray(_, _)
            ) {
                let pad = align_gap(size_accum, ALIGN_TO);
                if pad > 0 {
                    out.push(LaidOutField::Padding {
                        index: padding_idx,
                        size: pad,
                    });
                    padding_idx += 1;
                    size_accum += pad;
                }
            }

            out.push(LaidOutField::Real(field));
            size_accum += field_size;
        }

        // Tail padding.
        let tail = align_gap(size_accum, ALIGN_TO);
        if tail > 0 {
            out.push(LaidOutField::Padding {
                index: padding_idx,
                size: tail,
            });
            size_accum += tail;
        }

        let laid_out = LaidOutMsg {
            name: def.name.clone(),
            snake_name: def.snake_name.clone(),
            fields: out,
            constants: def.constants.clone(),
            topics: def.topics.clone(),
            size: size_accum,
        };
        self.cache.insert(def.name.clone(), laid_out.clone());
        Ok(laid_out)
    }

    fn find(&self, name: &str) -> Result<PathBuf, ParseError> {
        let file = format!("{name}.msg");
        for dir in &self.search {
            let p = dir.join(&file);
            if p.is_file() {
                return Ok(p);
            }
        }
        Err(ParseError::UnresolvedNested(name.to_string()))
    }

    /// Size used only for the stable sort. Scalars use their byte size;
    /// nested types get 0 so they sort to the end (PX4's convention).
    fn field_sort_size(&mut self, ty: &FieldType) -> Result<usize, ParseError> {
        Ok(match ty {
            FieldType::Scalar(s) | FieldType::ScalarArray(s, _) => s.size(),
            FieldType::Nested(_) | FieldType::NestedArray(_, _) => 0,
        })
    }

    /// Total byte size of a field (element size × array length).
    fn field_total_size(&mut self, ty: &FieldType) -> Result<usize, ParseError> {
        Ok(match ty {
            FieldType::Scalar(s) => s.size(),
            FieldType::ScalarArray(s, n) => s.size() * n,
            FieldType::Nested(name) => self.layout_by_name(name)?.size,
            FieldType::NestedArray(name, n) => self.layout_by_name(name)?.size * n,
        })
    }
}

fn align_gap(pos: usize, align: usize) -> usize {
    let rem = pos % align;
    if rem == 0 { 0 } else { align - rem }
}

pub fn rust_type_for(ty: &FieldType) -> String {
    match ty {
        FieldType::Scalar(s) => s.rust_type().to_string(),
        FieldType::ScalarArray(s, n) => format!("[{}; {}]", s.rust_type(), n),
        FieldType::Nested(name) => name.clone(),
        FieldType::NestedArray(name, n) => format!("[{name}; {n}]"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lay_out_from_text(name: &str, text: &str, search_dirs: Vec<PathBuf>) -> LaidOutMsg {
        let def = parser::parse_str(name, text).unwrap();
        let mut r = Resolver::new(search_dirs);
        r.layout(&def).unwrap()
    }

    fn dummy_path() -> PathBuf {
        std::env::temp_dir()
    }

    #[test]
    fn sensor_gyro_size_is_48() {
        // From the real PX4 SensorGyro.msg. Fields after sorting by size
        // descending (stable):
        //   u64 timestamp, u64 timestamp_sample   (8+8 = 16)
        //   u32 device_id, f32 x, f32 y, f32 z,
        //   f32 temperature, u32 error_count      (6 * 4 = 24)
        //   [u8;3] clip_counter, u8 samples       (3 + 1 = 4)
        //   tail pad to 48                        (4)
        // Total: 48 bytes.
        let text = "\
uint64 timestamp
uint64 timestamp_sample
uint32 device_id
float32 x
float32 y
float32 z
float32 temperature
uint32 error_count
uint8[3] clip_counter
uint8 samples
uint8 ORB_QUEUE_LENGTH = 8
";
        let laid = lay_out_from_text("SensorGyro", text, vec![dummy_path()]);
        assert_eq!(laid.size, 48, "SensorGyro size mismatch");

        // Verify field order.
        let real_names: Vec<_> = laid
            .fields
            .iter()
            .filter_map(|f| match f {
                LaidOutField::Real(r) => Some(r.name.as_str()),
                LaidOutField::Padding { .. } => None,
            })
            .collect();
        assert_eq!(
            real_names,
            &[
                "timestamp",
                "timestamp_sample",
                "device_id",
                "x",
                "y",
                "z",
                "temperature",
                "error_count",
                "clip_counter",
                "samples",
            ]
        );
    }

    #[test]
    fn actuator_outputs_with_topics() {
        let text = "\
uint64 timestamp
uint8 NUM_ACTUATOR_OUTPUTS = 16
uint32 noutputs
float32[16] output
# TOPICS actuator_outputs actuator_outputs_sim actuator_outputs_debug
";
        let laid = lay_out_from_text("ActuatorOutputs", text, vec![dummy_path()]);
        // Layout after sort:
        //   u64 timestamp            (8)
        //   [f32;16] output          (64)
        //   u32 noutputs             (4)
        //   tail pad                 (4)
        // Total: 80
        assert_eq!(laid.size, 80);
        assert_eq!(laid.topics.len(), 3);
    }
}
