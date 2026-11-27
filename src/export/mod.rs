//! Path exporters. Each format turns the document into a text file; add new
//! formats (Rust, Python, …) by implementing [`Exporter`].

mod json;

use anyhow::Result;

pub use json::JsonExporter;

use crate::model::Doc;

#[derive(Clone, Debug)]
pub struct ExportOptions {
    /// Include the path sampled at this step (mm), or skip samples when `None`.
    pub sample_step: Option<f64>,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self { sample_step: Some(10.) }
    }
}

pub trait Exporter {
    /// Shown in the UI.
    fn name(&self) -> &'static str;
    /// File extension without the dot.
    fn extension(&self) -> &'static str;
    fn export(&self, doc: &Doc, options: &ExportOptions) -> Result<String>;
}
