use bitflags::bitflags;
use crossfont::{GlyphKey, RasterizedGlyph};
use log::debug;

use alacritty_terminal::index::Point;
use alacritty_terminal::term::cell::{Flags, MathCharStyle, MathLayout};

use crate::display::SizeInfo;
use crate::display::content::RenderableCell;
use crate::gl;
use crate::gl::types::*;

mod atlas;
mod builtin_font;
mod gles2;
mod glsl3;
pub mod glyph_cache;

use atlas::Atlas;
pub use gles2::Gles2Renderer;
pub use glsl3::Glsl3Renderer;
pub use glyph_cache::GlyphCache;
use glyph_cache::{Glyph, LoadGlyph};

// NOTE: These flags must be in sync with their usage in the text.*.glsl shaders.
bitflags! {
    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    struct RenderingGlyphFlags: u8 {
        const COLORED   = 0b0000_0001;
        const WIDE_CHAR = 0b0000_0010;
    }
}

/// Rendering passes, for both GLES2 and GLSL3 renderer.
#[repr(u8)]
enum RenderingPass {
    /// Rendering pass used to render background color in text shaders.
    Background = 0,

    /// The first pass to render text with both GLES2 and GLSL3 renderers.
    SubpixelPass1 = 1,

    /// The second pass to render text with GLES2 renderer.
    SubpixelPass2 = 2,

    /// The third pass to render text with GLES2 renderer.
    SubpixelPass3 = 3,
}

pub trait TextRenderer<'a> {
    type Shader: TextShader;
    type RenderBatch: TextRenderBatch;
    type RenderApi: TextRenderApi<Self::RenderBatch>;

    /// Get loader API for the renderer.
    fn loader_api(&mut self) -> LoaderApi<'_>;

    /// Draw cells.
    fn draw_cells<'b: 'a, I: Iterator<Item = RenderableCell>>(
        &'b mut self,
        size_info: &'b SizeInfo,
        glyph_cache: &'a mut GlyphCache,
        cells: I,
    ) {
        self.with_api(size_info, |mut api| {
            for cell in cells {
                api.draw_cell(cell, glyph_cache, size_info);
            }
        })
    }

    fn with_api<'b: 'a, F, T>(&'b mut self, size_info: &'b SizeInfo, func: F) -> T
    where
        F: FnOnce(Self::RenderApi) -> T;

    fn program(&self) -> &Self::Shader;

    /// Resize the text rendering.
    fn resize(&self, size: &SizeInfo) {
        unsafe {
            let program = self.program();
            gl::UseProgram(program.id());
            update_projection(program.projection_uniform(), size);
            gl::UseProgram(0);
        }
    }

    /// Invoke renderer with the loader.
    fn with_loader<F: FnOnce(LoaderApi<'_>) -> T, T>(&mut self, func: F) -> T {
        unsafe {
            gl::ActiveTexture(gl::TEXTURE0);
        }

        func(self.loader_api())
    }
}

pub trait TextRenderBatch {
    /// Check if `Batch` is empty.
    fn is_empty(&self) -> bool;

    /// Check whether the `Batch` is full.
    fn full(&self) -> bool;

    /// Get texture `Batch` is using.
    fn tex(&self) -> GLuint;

    /// Add item to the batch.
    fn add_item(&mut self, cell: &RenderableCell, glyph: &Glyph, size_info: &SizeInfo);
}

pub trait TextRenderApi<T: TextRenderBatch>: LoadGlyph {
    /// Get `Batch` the api is using.
    fn batch(&mut self) -> &mut T;

    /// Render the underlying data.
    fn render_batch(&mut self);

    /// Add item to the rendering queue.
    #[inline]
    fn add_render_item(&mut self, cell: &RenderableCell, glyph: &Glyph, size_info: &SizeInfo) {
        // Flush batch if tex changing.
        if !self.batch().is_empty() && self.batch().tex() != glyph.tex_id {
            self.render_batch();
        }

        self.batch().add_item(cell, glyph, size_info);

        // Render batch and clear if it's full.
        if self.batch().full() {
            self.render_batch();
        }
    }

    /// Draw cell.
    fn draw_cell(
        &mut self,
        mut cell: RenderableCell,
        glyph_cache: &mut GlyphCache,
        size_info: &SizeInfo,
    ) {
        // Get font key for cell.
        let font_key = match cell.flags & Flags::BOLD_ITALIC {
            Flags::BOLD_ITALIC => glyph_cache.bold_italic_key,
            Flags::ITALIC => glyph_cache.italic_key,
            Flags::BOLD => glyph_cache.bold_key,
            _ => glyph_cache.font_key,
        };

        // Ignore hidden cells and render tabs as spaces to prevent font issues.
        let hidden = cell.flags.contains(Flags::HIDDEN);
        if cell.character == '\t' || hidden {
            cell.character = ' ';
        }

        let mut glyph_key =
            GlyphKey { font_key, size: glyph_cache.font_size, character: cell.character };

        // Add cell to batch.
        let glyph = glyph_cache.get(glyph_key, self, true);
        self.add_render_item(&cell, &glyph, size_info);

        // Render math formula with complex layout.
        if cell.flags.contains(Flags::MATH_FORMULA) {
            debug!("MATH_FORMULA flag detected, extra: {:?}", cell.extra.is_some());
            if let Some(layout) = cell.extra.as_mut().and_then(|extra| extra.math_layout.take()) {
                debug!("MathLayout found: {:?}", layout);
                let cell_height = size_info.cell_height() as i16;
                let cell_width = size_info.cell_width() as i16;

                match layout {
                    MathLayout::Fraction { numerator, denominator } => {
                        // Note: crossfont ignores size in GlyphKey, so we manually scale glyphs.
                        // All chars share same column, use cumulative left offset for compact layout.
                        let y_offset = (cell_height * 2) / 5;
                        let scale = 0.7f32;

                        // Render numerator (above the fraction line, 70% size).
                        let mut x_offset = 0i16;
                        for &ch in numerator.iter() {
                            glyph_key.character = ch;
                            let mut glyph = glyph_cache.get(glyph_key, self, false);
                            // Manually scale glyph dimensions.
                            let scaled_width = (glyph.width as f32 * scale) as i16;
                            glyph.width = scaled_width;
                            glyph.height = (glyph.height as f32 * scale) as i16;
                            glyph.top = (glyph.top as f32 * scale) as i16 + y_offset;
                            glyph.left = (glyph.left as f32 * scale) as i16 + x_offset;
                            let offset_cell = RenderableCell {
                                character: ch,
                                point: Point::new(cell.point.line, cell.point.column),
                                fg: cell.fg,
                                bg: cell.bg,
                                bg_alpha: 0.,
                                underline: cell.underline,
                                flags: cell.flags & !Flags::MATH_FORMULA,
                                extra: None,
                            };
                            self.add_render_item(&offset_cell, &glyph, size_info);
                            x_offset += scaled_width;
                        }

                        // Render denominator (below the fraction line, 70% size).
                        let mut x_offset = 0i16;
                        for &ch in denominator.iter() {
                            glyph_key.character = ch;
                            let mut glyph = glyph_cache.get(glyph_key, self, false);
                            // Manually scale glyph dimensions.
                            let scaled_width = (glyph.width as f32 * scale) as i16;
                            glyph.width = scaled_width;
                            glyph.height = (glyph.height as f32 * scale) as i16;
                            glyph.top = (glyph.top as f32 * scale) as i16 - y_offset;
                            glyph.left = (glyph.left as f32 * scale) as i16 + x_offset;
                            let offset_cell = RenderableCell {
                                character: ch,
                                point: Point::new(cell.point.line, cell.point.column),
                                fg: cell.fg,
                                bg: cell.bg,
                                bg_alpha: 0.,
                                underline: cell.underline,
                                flags: cell.flags & !Flags::MATH_FORMULA,
                                extra: None,
                            };
                            self.add_render_item(&offset_cell, &glyph, size_info);
                            x_offset += scaled_width;
                        }
                        // Fraction line is already rendered as the main cell character.
                    },
                    MathLayout::Superscript { base, script } => {
                        // Render base characters (skip first, already rendered).
                        for (i, &ch) in base.iter().skip(1).enumerate() {
                            glyph_key.character = ch;
                            let glyph = glyph_cache.get(glyph_key, self, false);
                            let offset_cell = RenderableCell {
                                character: ch,
                                point: Point::new(cell.point.line, cell.point.column + i + 1),
                                fg: cell.fg,
                                bg: cell.bg,
                                bg_alpha: 0.,
                                underline: cell.underline,
                                flags: cell.flags & !Flags::MATH_FORMULA,
                                extra: None,
                            };
                            self.add_render_item(&offset_cell, &glyph, size_info);
                        }

                        // Render script characters (raised, 70% size).
                        // Note: crossfont ignores size in GlyphKey, so we manually scale the glyph.
                        // All script chars share same column, use cumulative left offset for compact layout.
                        let y_offset = (cell_height * 2) / 5;
                        let scale = 0.7f32;
                        let mut x_offset = 0i16;
                        for &ch in script.iter() {
                            glyph_key.character = ch;
                            let mut glyph = glyph_cache.get(glyph_key, self, false);
                            // Manually scale glyph dimensions.
                            let scaled_width = (glyph.width as f32 * scale) as i16;
                            glyph.width = scaled_width;
                            glyph.height = (glyph.height as f32 * scale) as i16;
                            glyph.top = (glyph.top as f32 * scale) as i16 + y_offset;
                            glyph.left = (glyph.left as f32 * scale) as i16 + x_offset;
                            let offset_cell = RenderableCell {
                                character: ch,
                                point: Point::new(cell.point.line, cell.point.column + base.len()),
                                fg: cell.fg,
                                bg: cell.bg,
                                bg_alpha: 0.,
                                underline: cell.underline,
                                flags: cell.flags & !Flags::MATH_FORMULA,
                                extra: None,
                            };
                            self.add_render_item(&offset_cell, &glyph, size_info);
                            x_offset += scaled_width;
                        }
                    },
                    MathLayout::Subscript { base, script } => {
                        // Render base characters (skip first, already rendered).
                        for (i, &ch) in base.iter().skip(1).enumerate() {
                            glyph_key.character = ch;
                            let glyph = glyph_cache.get(glyph_key, self, false);
                            let offset_cell = RenderableCell {
                                character: ch,
                                point: Point::new(cell.point.line, cell.point.column + i + 1),
                                fg: cell.fg,
                                bg: cell.bg,
                                bg_alpha: 0.,
                                underline: cell.underline,
                                flags: cell.flags & !Flags::MATH_FORMULA,
                                extra: None,
                            };
                            self.add_render_item(&offset_cell, &glyph, size_info);
                        }

                        // Render script characters (lowered, 70% size).
                        // Note: crossfont ignores size in GlyphKey, so we manually scale the glyph.
                        // All script chars share same column, use cumulative left offset for compact layout.
                        let y_offset = (cell_height * 2) / 5;
                        let scale = 0.7f32;
                        let mut x_offset = 0i16;
                        for &ch in script.iter() {
                            glyph_key.character = ch;
                            let mut glyph = glyph_cache.get(glyph_key, self, false);
                            // Manually scale glyph dimensions.
                            let scaled_width = (glyph.width as f32 * scale) as i16;
                            glyph.width = scaled_width;
                            glyph.height = (glyph.height as f32 * scale) as i16;
                            glyph.top = (glyph.top as f32 * scale) as i16 - y_offset;
                            glyph.left = (glyph.left as f32 * scale) as i16 + x_offset;
                            let offset_cell = RenderableCell {
                                character: ch,
                                point: Point::new(cell.point.line, cell.point.column + base.len()),
                                fg: cell.fg,
                                bg: cell.bg,
                                bg_alpha: 0.,
                                underline: cell.underline,
                                flags: cell.flags & !Flags::MATH_FORMULA,
                                extra: None,
                            };
                            self.add_render_item(&offset_cell, &glyph, size_info);
                            x_offset += scaled_width;
                        }
                    },
                    MathLayout::Sqrt { index, content } => {
                        // For n-th root, render index as small superscript before √.
                        let mut content_x_offset = 0i16;
                        if let Some(idx) = index {
                            let scale = 0.6f32;
                            let y_offset = -(cell_height / 3);
                            let mut x_off = -(cell_width / 3);
                            for &ch in idx.iter() {
                                glyph_key.character = ch;
                                let mut glyph = glyph_cache.get(glyph_key, self, false);
                                let scaled_width = (glyph.width as f32 * scale) as i16;
                                glyph.width = scaled_width;
                                glyph.height = (glyph.height as f32 * scale) as i16;
                                glyph.top = (glyph.top as f32 * scale) as i16 - y_offset;
                                glyph.left = (glyph.left as f32 * scale) as i16 + x_off;
                                let offset_cell = RenderableCell {
                                    character: ch,
                                    point: cell.point,
                                    fg: cell.fg,
                                    bg: cell.bg,
                                    bg_alpha: 0.,
                                    underline: cell.underline,
                                    flags: cell.flags & !Flags::MATH_FORMULA,
                                    extra: None,
                                };
                                self.add_render_item(&offset_cell, &glyph, size_info);
                                x_off += scaled_width;
                            }
                            // Adjust content offset if index is wide.
                            content_x_offset = x_off.max(0);
                        }

                        // Sqrt symbol is already rendered as main character.
                        // Render content characters after the sqrt symbol.
                        for (i, &ch) in content.iter().enumerate() {
                            glyph_key.character = ch;
                            let mut glyph = glyph_cache.get(glyph_key, self, false);
                            glyph.left += content_x_offset;
                            let offset_cell = RenderableCell {
                                character: ch,
                                point: Point::new(cell.point.line, cell.point.column + i + 1),
                                fg: cell.fg,
                                bg: cell.bg,
                                bg_alpha: 0.,
                                underline: cell.underline,
                                flags: cell.flags & !Flags::MATH_FORMULA,
                                extra: None,
                            };
                            self.add_render_item(&offset_cell, &glyph, size_info);
                        }
                    },
                    MathLayout::SubSuperscript { base, lower, upper } => {
                        // Render remaining base characters (skip first, already rendered).
                        for (i, &ch) in base.iter().skip(1).enumerate() {
                            glyph_key.character = ch;
                            let glyph = glyph_cache.get(glyph_key, self, false);
                            let offset_cell = RenderableCell {
                                character: ch,
                                point: Point::new(cell.point.line, cell.point.column + i + 1),
                                fg: cell.fg,
                                bg: cell.bg,
                                bg_alpha: 0.,
                                underline: cell.underline,
                                flags: cell.flags & !Flags::MATH_FORMULA,
                                extra: None,
                            };
                            self.add_render_item(&offset_cell, &glyph, size_info);
                        }

                        // Render lower script (subscript, 70% size, lowered).
                        // All script chars share same column, use cumulative left offset.
                        let y_offset = (cell_height * 2) / 5;
                        let scale = 0.7f32;
                        let script_column = cell.point.column + base.len();
                        let mut x_offset = 0i16;
                        for &ch in lower.iter() {
                            glyph_key.character = ch;
                            let mut glyph = glyph_cache.get(glyph_key, self, false);
                            let scaled_width = (glyph.width as f32 * scale) as i16;
                            glyph.width = scaled_width;
                            glyph.height = (glyph.height as f32 * scale) as i16;
                            glyph.top = (glyph.top as f32 * scale) as i16 - y_offset;
                            glyph.left = (glyph.left as f32 * scale) as i16 + x_offset;
                            let offset_cell = RenderableCell {
                                character: ch,
                                point: Point::new(cell.point.line, script_column),
                                fg: cell.fg,
                                bg: cell.bg,
                                bg_alpha: 0.,
                                underline: cell.underline,
                                flags: cell.flags & !Flags::MATH_FORMULA,
                                extra: None,
                            };
                            self.add_render_item(&offset_cell, &glyph, size_info);
                            x_offset += scaled_width;
                        }

                        // Render upper script (superscript, 70% size, raised).
                        // Reset x_offset so upper script also starts right after base symbol.
                        let mut x_offset = 0i16;
                        for &ch in upper.iter() {
                            glyph_key.character = ch;
                            let mut glyph = glyph_cache.get(glyph_key, self, false);
                            let scaled_width = (glyph.width as f32 * scale) as i16;
                            glyph.width = scaled_width;
                            glyph.height = (glyph.height as f32 * scale) as i16;
                            glyph.top = (glyph.top as f32 * scale) as i16 + y_offset;
                            glyph.left = (glyph.left as f32 * scale) as i16 + x_offset;
                            let offset_cell = RenderableCell {
                                character: ch,
                                point: Point::new(cell.point.line, script_column),
                                fg: cell.fg,
                                bg: cell.bg,
                                bg_alpha: 0.,
                                underline: cell.underline,
                                flags: cell.flags & !Flags::MATH_FORMULA,
                                extra: None,
                            };
                            self.add_render_item(&offset_cell, &glyph, size_info);
                            x_offset += scaled_width;
                        }
                    },
                }
            } else if let Some(math_chars) =
                cell.extra.as_mut().and_then(|extra| extra.math_chars.take().filter(|_| !hidden))
            {
                // Get styles if available.
                let math_styles = cell.extra.as_mut().and_then(|extra| extra.math_styles.take());

                // Simple formula: render characters with X offset and scaling based on style.
                let cell_height = size_info.cell_height() as i16;
                let scale = 0.7f32;
                let sub_y_offset = (cell_height as f32 * 0.3) as i16;
                let sup_y_offset = -(cell_height as f32 * 0.4) as i16;

                let mut x_offset = 0i16;
                for (i, character) in math_chars.into_iter().enumerate() {
                    glyph_key.character = character;
                    let base_glyph = glyph_cache.get(glyph_key, self, false);

                    // Check style for this character.
                    let style = math_styles
                        .as_ref()
                        .and_then(|s| s.get(i).copied())
                        .unwrap_or(MathCharStyle::Normal);

                    let glyph = match style {
                        MathCharStyle::Subscript => {
                            let mut g = base_glyph;
                            let scaled_width = (g.width as f32 * scale) as i16;
                            g.width = scaled_width;
                            g.height = (g.height as f32 * scale) as i16;
                            g.top = (g.top as f32 * scale) as i16 + sub_y_offset;
                            g.left = (g.left as f32 * scale) as i16 + x_offset;
                            x_offset += scaled_width;
                            g
                        },
                        MathCharStyle::Superscript => {
                            let mut g = base_glyph;
                            let scaled_width = (g.width as f32 * scale) as i16;
                            g.width = scaled_width;
                            g.height = (g.height as f32 * scale) as i16;
                            g.top = (g.top as f32 * scale) as i16 + sup_y_offset;
                            g.left = (g.left as f32 * scale) as i16 + x_offset;
                            x_offset += scaled_width;
                            g
                        },
                        MathCharStyle::Normal | MathCharStyle::Bold => {
                            let mut g = base_glyph;
                            g.left += x_offset;
                            x_offset += g.width;
                            g
                        },
                    };

                    // All styled chars share the same column, with x_offset handling positioning.
                    let offset_cell = RenderableCell {
                        character,
                        point: Point::new(cell.point.line, cell.point.column + 1),
                        fg: cell.fg,
                        bg: cell.bg,
                        bg_alpha: 0.,
                        underline: cell.underline,
                        flags: cell.flags & !Flags::MATH_FORMULA,
                        extra: None,
                    };
                    self.add_render_item(&offset_cell, &glyph, size_info);
                }
            }
        }

        // Render visible zero-width characters.
        if let Some(zerowidth) =
            cell.extra.as_mut().and_then(|extra| extra.zerowidth.take().filter(|_| !hidden))
        {
            for character in zerowidth {
                glyph_key.character = character;
                let glyph = glyph_cache.get(glyph_key, self, false);
                self.add_render_item(&cell, &glyph, size_info);
            }
        }
    }
}

pub trait TextShader {
    fn id(&self) -> GLuint;

    /// Id of the projection uniform.
    fn projection_uniform(&self) -> GLint;
}

#[derive(Debug)]
pub struct LoaderApi<'a> {
    active_tex: &'a mut GLuint,
    atlas: &'a mut Vec<Atlas>,
    current_atlas: &'a mut usize,
}

impl LoadGlyph for LoaderApi<'_> {
    fn load_glyph(&mut self, rasterized: &RasterizedGlyph) -> Glyph {
        Atlas::load_glyph(self.active_tex, self.atlas, self.current_atlas, rasterized)
    }

    fn clear(&mut self) {
        Atlas::clear_atlas(self.atlas, self.current_atlas)
    }
}

fn update_projection(u_projection: GLint, size: &SizeInfo) {
    let width = size.width();
    let height = size.height();
    let padding_x = size.padding_x();
    let padding_y = size.padding_y();

    // Bounds check.
    if (width as u32) < (2 * padding_x as u32) || (height as u32) < (2 * padding_y as u32) {
        return;
    }

    // Compute scale and offset factors, from pixel to ndc space. Y is inverted.
    //   [0, width - 2 * padding_x] to [-1, 1]
    //   [height - 2 * padding_y, 0] to [-1, 1]
    let scale_x = 2. / (width - 2. * padding_x);
    let scale_y = -2. / (height - 2. * padding_y);
    let offset_x = -1.;
    let offset_y = 1.;

    unsafe {
        gl::Uniform4f(u_projection, offset_x, offset_y, scale_x, scale_y);
    }
}
