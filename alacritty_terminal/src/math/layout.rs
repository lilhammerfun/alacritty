//! Layout extraction from Typst Frame.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use typst::layout::{Frame, FrameItem, Point};

/// A positioned layout item extracted from Typst Frame.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct LayoutItem {
    /// Position in points (pt).
    pub x: f64,
    pub y: f64,
    /// Content at this position.
    pub content: LayoutContent,
}

// Implement Eq using bit comparison for f64 fields.
impl Eq for LayoutItem {}

/// Content types that can be rendered.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum LayoutContent {
    /// Text with font size.
    Text { text: String, font_size: f64 },
    /// A line (e.g., fraction bar).
    Line { dx: f64, dy: f64 },
    /// A rectangle.
    Rect { width: f64, height: f64 },
}

impl Eq for LayoutContent {}

/// Layout information for a complete math formula.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct TypstMathLayout {
    /// Positioned layout items.
    pub items: Vec<LayoutItem>,
    /// Frame width in points.
    pub width_pt: f64,
    /// Frame height in points.
    pub height_pt: f64,
}

impl Eq for TypstMathLayout {}

/// Extract layout items from a Typst Frame.
///
/// Recursively traverses the Frame and collects all renderable items
/// with their absolute positions.
pub fn extract_layout(frame: &Frame) -> Vec<LayoutItem> {
    extract_layout_recursive(frame, Point::zero())
}

fn extract_layout_recursive(frame: &Frame, offset: Point) -> Vec<LayoutItem> {
    let mut items = Vec::new();

    for (point, item) in frame.items() {
        let abs_x = offset.x + point.x;
        let abs_y = offset.y + point.y;

        match item {
            FrameItem::Text(text) => {
                items.push(LayoutItem {
                    x: abs_x.to_pt(),
                    y: abs_y.to_pt(),
                    content: LayoutContent::Text {
                        text: text.text.to_string(),
                        font_size: text.size.to_pt(),
                    },
                });
            }
            FrameItem::Shape(shape, _span) => {
                use typst::visualize::Geometry;
                match &shape.geometry {
                    Geometry::Line(to) => {
                        items.push(LayoutItem {
                            x: abs_x.to_pt(),
                            y: abs_y.to_pt(),
                            content: LayoutContent::Line {
                                dx: to.x.to_pt(),
                                dy: to.y.to_pt(),
                            },
                        });
                    }
                    Geometry::Rect(size) => {
                        items.push(LayoutItem {
                            x: abs_x.to_pt(),
                            y: abs_y.to_pt(),
                            content: LayoutContent::Rect {
                                width: size.x.to_pt(),
                                height: size.y.to_pt(),
                            },
                        });
                    }
                    _ => {}
                }
            }
            FrameItem::Group(group) => {
                let nested_offset = Point::new(abs_x, abs_y);
                let nested_items = extract_layout_recursive(&group.frame, nested_offset);
                items.extend(nested_items);
            }
            _ => {}
        }
    }

    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_item_debug() {
        let item = LayoutItem {
            x: 10.0,
            y: 20.0,
            content: LayoutContent::Text {
                text: "a".to_string(),
                font_size: 11.0,
            },
        };
        assert!(format!("{:?}", item).contains("10.0"));
    }
}
