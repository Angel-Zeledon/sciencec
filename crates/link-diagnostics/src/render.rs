//! STUB: lo implementa el agente de diagnósticos.
use crate::{Diagnostic, SourceMap};

pub fn render(_map: &SourceMap, d: &Diagnostic) -> String {
    format!("error[{}]: {}", d.code, d.message)
}
