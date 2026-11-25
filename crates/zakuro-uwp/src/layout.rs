//! where the two screens go on a TV.

/// a rectangle in the window's pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }
}

/// the arrangements View + LB cycles through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Layout {
    /// the top screen big, the bottom one at half its scale beside it, the
    /// best use of a 16:9 TV.
    #[default]
    LargeTop,
    /// both at the same scale, side by side.
    SideBySide,
    /// the bottom screen under the top one, as on the console.
    Stacked,
    /// the top screen alone, filling the TV.
    TopOnly,
}

impl Layout {
    pub fn next(self) -> Layout {
        match self {
            Layout::LargeTop => Layout::SideBySide,
            Layout::SideBySide => Layout::Stacked,
            Layout::Stacked => Layout::TopOnly,
            Layout::TopOnly => Layout::LargeTop,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Layout::LargeTop => "Large top screen",
            Layout::SideBySide => "Side by side",
            Layout::Stacked => "Stacked",
            Layout::TopOnly => "Top screen only",
        }
    }

    /// where the top and bottom screens go in a window of this size,
    /// centered, keeping their shape. none for the bottom one when it isn't
    /// shown.
    pub fn place(self, width: f32, height: f32) -> (Rect, Option<Rect>) {
        const TOP: (f32, f32) = (400.0, 240.0);
        const BOTTOM: (f32, f32) = (320.0, 240.0);
        // the space the arrangement takes at scale 1, and the bottom
        // screen's scale against the top one's
        let (span, bottom_scale, beside) = match self {
            Layout::LargeTop => ((TOP.0 + BOTTOM.0 / 2.0, TOP.1), 0.5, true),
            Layout::SideBySide => ((TOP.0 + BOTTOM.0, TOP.1), 1.0, true),
            Layout::Stacked => ((TOP.0, TOP.1 + BOTTOM.1), 1.0, false),
            Layout::TopOnly => (TOP, 0.0, true),
        };
        let scale = (width / span.0).min(height / span.1).max(0.0);
        let x = (width - span.0 * scale) / 2.0;
        let y = (height - span.1 * scale) / 2.0;
        let top = Rect { x, y, width: TOP.0 * scale, height: TOP.1 * scale };
        if self == Layout::TopOnly {
            return (top, None);
        }
        let (bottom_width, bottom_height) = (BOTTOM.0 * scale * bottom_scale, BOTTOM.1 * scale * bottom_scale);
        let bottom = if beside {
            // level with the top screen's bottom edge, as a handheld's
            // screens sit
            Rect { x: top.right(), y: top.bottom() - bottom_height, width: bottom_width, height: bottom_height }
        } else {
            Rect { x: x + (top.width - bottom_width) / 2.0, y: top.bottom(), width: bottom_width, height: bottom_height }
        };
        (top, Some(bottom))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_layout_fits_a_1080p_tv() {
        let mut layout = Layout::default();
        for _ in 0..4 {
            let (top, bottom) = layout.place(1920.0, 1080.0);
            for rect in [Some(top), bottom].into_iter().flatten() {
                assert!(rect.x >= -0.01 && rect.y >= -0.01, "{layout:?} {rect:?}");
                assert!(rect.right() <= 1920.01 && rect.bottom() <= 1080.01, "{layout:?} {rect:?}");
                assert!((rect.width / rect.height - if rect.width / rect.height > 1.5 { 400.0 / 240.0 } else { 320.0 / 240.0 }).abs() < 0.01);
            }
            layout = layout.next();
        }
        assert_eq!(layout, Layout::default(), "the cycle comes back around");
    }

    #[test]
    fn the_large_top_layout_uses_the_full_width() {
        let (top, bottom) = Layout::LargeTop.place(1920.0, 1080.0);
        let bottom = bottom.unwrap();
        assert!((bottom.right() - 1920.0).abs() < 0.01);
        assert!((top.width - bottom.width * 2.5).abs() < 0.01);
    }
}
