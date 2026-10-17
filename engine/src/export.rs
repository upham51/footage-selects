//! Timeline export. Filled in by the export writers.

use crate::project::Project;
use std::path::{Path, PathBuf};

pub struct Written {
    pub files: Vec<PathBuf>,
    pub selects: usize,
    pub warnings: Vec<String>,
}

pub fn write_all(_project: &Project, _base: &Path) -> Result<Written, String> {
    Err("Export is not available in this build yet. Nothing was written.".into())
}
