//! Path exporters. Each format turns the document into a text file; add new
//! formats (Rust, Python, …) by implementing [`Exporter`].

mod json;

use anyhow::Result;

pub use json::JsonExporter;

use crate::model::Doc;

pub trait Exporter {
    /// Shown in the UI.
    fn name(&self) -> &'static str;
    /// File extension without the dot.
    fn extension(&self) -> &'static str;
    fn export(&self, doc: &Doc) -> Result<String>;
}
