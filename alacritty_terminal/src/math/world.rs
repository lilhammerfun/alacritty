//! Minimal Typst World implementation for math rendering.

use std::sync::OnceLock;

use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime};
use typst::layout::Frame;
use typst::syntax::{FileId, Source};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_library::layout::PagedDocument;

/// Global font cache to avoid reloading fonts for each compilation.
static FONTS: OnceLock<Vec<Font>> = OnceLock::new();
static FONT_BOOK: OnceLock<LazyHash<FontBook>> = OnceLock::new();

/// Minimal World implementation for math-only rendering.
struct MathWorld {
    library: LazyHash<Library>,
    source: Source,
}

impl MathWorld {
    fn new(source_code: &str) -> Self {
        // Initialize fonts once.
        FONTS.get_or_init(Self::load_fonts);
        FONT_BOOK.get_or_init(|| {
            let fonts = FONTS.get().unwrap();
            LazyHash::new(FontBook::from_fonts(fonts.iter()))
        });

        Self {
            library: LazyHash::new(Library::default()),
            source: Source::detached(source_code),
        }
    }

    fn load_fonts() -> Vec<Font> {
        let mut fonts = Vec::new();
        for data in typst_assets::fonts() {
            let buffer = Bytes::new(data.to_vec());
            for font in Font::iter(buffer) {
                fonts.push(font);
            }
        }
        fonts
    }
}

impl World for MathWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        FONT_BOOK.get().unwrap()
    }

    fn main(&self) -> FileId {
        self.source.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.source.id() {
            Ok(self.source.clone())
        } else {
            Err(FileError::NotFound(id.vpath().as_rootless_path().into()))
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        Err(FileError::NotFound(id.vpath().as_rootless_path().into()))
    }

    fn font(&self, index: usize) -> Option<Font> {
        FONTS.get()?.get(index).cloned()
    }

    fn today(&self, _offset: Option<i64>) -> Option<Datetime> {
        None
    }
}

/// Compile Typst math source and return the Frame.
///
/// Takes Typst math syntax (e.g., "frac(a, b)") and returns the layout Frame.
pub fn compile_math(typst_math: &str) -> Result<Frame, Vec<String>> {
    let source = format!("$ {} $", typst_math);
    let world = MathWorld::new(&source);

    let result = typst::compile::<PagedDocument>(&world);

    match result.output {
        Ok(document) => {
            if let Some(page) = document.pages.first() {
                Ok(page.frame.clone())
            } else {
                Err(vec!["No pages in document".to_string()])
            }
        }
        Err(errors) => Err(errors.iter().map(|e| format!("{:?}", e)).collect()),
    }
}

/// Compile Typst math source and return the PagedDocument for rendering.
///
/// `font_size_pt` sets the base font size in points (e.g., 16.0 for 16pt).
pub fn compile_math_document(typst_math: &str, font_size_pt: f32, fg_color: [u8; 3]) -> Result<PagedDocument, Vec<String>> {
    // Use auto page size to fit content exactly.
    // Set font size to match terminal font.
    // Use transparent background and terminal foreground color.
    let source = format!(
        "#set page(width: auto, height: auto, margin: 0pt, fill: none)\n#set text(size: {}pt, fill: rgb({}, {}, {}))\n$ {} $",
        font_size_pt, fg_color[0], fg_color[1], fg_color[2], typst_math
    );
    let world = MathWorld::new(&source);

    let result = typst::compile::<PagedDocument>(&world);

    match result.output {
        Ok(document) => Ok(document),
        Err(errors) => Err(errors.iter().map(|e| format!("{:?}", e)).collect()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_frac() {
        let frame = compile_math("frac(a, b)").expect("Compilation failed");
        assert!(frame.width().to_pt() > 0.0);
        assert!(frame.height().to_pt() > 0.0);
    }
}
