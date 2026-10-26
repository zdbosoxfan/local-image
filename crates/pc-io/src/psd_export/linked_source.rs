use super::*;

impl Ex {
    /// The `lnk2` file a smart object's source is written as: a preserved embedded file, or a new
    /// one (a `.pcraft` source becomes a PSB of the nested document; image files are embedded as
    /// they are; a linked file is read and embedded).
    pub(super) fn smart_source(&mut self, sm: &photocraft_doc::SmartObject, template: Option<&crate::smart_map::Placed>) -> Result<Source, String> {
        use photocraft_doc::SmartSource;
        match &sm.source {
            // A file of the imported document, or one its placed-layer data names (a missing
            // link stays a smart object, as in Photoshop).
            SmartSource::Linked { path } if self.smart.known.contains(path) || template.is_some_and(|t| t.idnt == *path) => {
                let stored = template.filter(|t| t.idnt == *path).and_then(|t| crate::smart_map::stored_size(&t.descriptor));
                let geometry = stored.map(|size| (size, f64::from(self.dpi))).or_else(|| {
                    let meta = photocraft_doc::Metadata { psd_global_blocks: self.smart.globals.clone(), ..Default::default() };
                    let f = crate::linked::find_linked_file(&meta, path)?;
                    source_geometry(&f.file_name, &f.bytes).ok()
                });
                let (size, dpi) = geometry.unwrap_or((size_from_layer(sm), f64::from(self.dpi)));
                Ok(Source { uuid: path.clone(), size, dpi })
            }
            SmartSource::Linked { path } => {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    if !self.read_linked_files {
                        return Err(format!("the linked file {path} is not read for this export"));
                    }
                    // The reader every open uses: it refuses device, kernel and network paths,
                    // non-regular files and files past the input limit.
                    let bytes = photocraft_format::read_file(std::path::Path::new(path)).map_err(|e| format!("the linked file {path} can't be read: {e}"))?;
                    let name = path.rsplit(['/', '\\']).next().unwrap_or(path).to_string();
                    let source = self.embed(&name, &bytes, true)?;
                    self.warnings.push(format!("the linked smart object {name} was embedded (PSD export keeps no external links)"));
                    Ok(source)
                }
                #[cfg(target_arch = "wasm32")]
                Err(format!("the linked file {path} can't be read here"))
            }
            SmartSource::Embedded { file_name, bytes } => self.embed(file_name, bytes, false),
        }
    }
}
