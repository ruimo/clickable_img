use bit_set::BitSet;
use egui::{ColorImage, Color32, TextureHandle, Vec2, Context, TextureFilter, Rect, Pos2};
use tiny_skia::{PixmapPaint, Transform};

#[derive(PartialEq, Clone, Debug)]
pub struct Pixels2D {
    bits: BitSet,
    rect: Rect,
}

impl Pixels2D {
    fn new(bits: BitSet, rect: Rect) -> Self {
        Self {
            bits, rect,
        }
    }

    #[inline]
    pub fn pixel_at(&self, x: usize, y: usize) -> bool {
        self.bits.contains(x + y * (self.rect.width() as usize))
    }

    pub fn pixel_count(&self, rect: Rect) -> usize {
        let start_x = rect.min.x as usize;
        let start_y = rect.min.y as usize;
        let w = rect.width() as usize;
        let h = rect.height() as usize;
        let mut count: usize = 0;

        for y in start_y..(start_y + h) {
            for x in start_x..(start_x + w) {
                if self.pixel_at(x, y) { count += 1; }
            }
        }

        count        
    }

    pub fn contains_pixel(&self, rect: &Rect) -> bool {
        let covered_both = rect.intersect(self.rect);
        if covered_both == Rect::NOTHING { return false; }

        let start_x = covered_both.min.x as usize;
        let start_y = covered_both.min.y as usize;
        let w = covered_both.width() as usize;
        let h = covered_both.height() as usize;

        for y in start_y..(start_y + h) {
            for x in start_x..(start_x + w) {
                if self.pixel_at(x, y) { return true; }
            }
        }

        return false;
    }
}

#[derive(PartialEq, Debug, Clone)]
pub enum LayeredRect {
    Leaf { rect: Rect, pixel_count: usize },
    Node { rect: Rect, children: [Box<LayeredRect>; 2], pixel_count: usize },
}

pub fn split_horizontal(rect: &Rect) -> [Rect; 2] {
    let split_at = (rect.width() as usize) / 2;
    let left_rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(split_at as f32, rect.height()));
    let right_rect = Rect::from_min_size(
        Pos2::new(split_at as f32, 0.),
        Vec2::new(rect.width() - split_at as f32, rect.height())
    );

    [ left_rect, right_rect ]
}

pub fn split_vertical(rect: &Rect) -> [Rect; 2] {
    let split_at = (rect.height() as usize) / 2;
    let top_rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(rect.width(), split_at as f32));
    let bottom_rect = Rect::from_min_size(
        Pos2::new(0., split_at as f32),
        Vec2::new(rect.width(), rect.height() - split_at as f32)
    );

    [ top_rect, bottom_rect ]
}

const MIN_NODE_SIZE: f32 = 3.;

impl LayeredRect {
    fn new(rect: Rect, bit_img: &Pixels2D) -> Self {
        if MIN_NODE_SIZE < rect.width() {
            let [b0, b1] = split_horizontal(&rect);
            let l0 = LayeredRect::new(b0, bit_img);
            let l1 = LayeredRect::new(b1, bit_img);
            LayeredRect::Node {
                rect,
                pixel_count: l0.pixel_count() + l1.pixel_count(),
                children: [Box::new(l0), Box::new(l1)]
            }
        } else if MIN_NODE_SIZE < rect.height() {
            let [b0, b1] = split_vertical(&rect);
            let l0 = LayeredRect::new(b0, bit_img);
            let l1 = LayeredRect::new(b1, bit_img);
            LayeredRect::Node {
                rect,
                pixel_count: l0.pixel_count() + l1.pixel_count(),
                children: [Box::new(l0), Box::new(l1)]
            }
        } else {
            LayeredRect::Leaf {
                rect,
                pixel_count: bit_img.pixel_count(rect),
            }
        }
    }

    fn pixel_count(&self) -> usize {
        match self {
            LayeredRect::Leaf { rect: _, pixel_count } => *pixel_count,
            LayeredRect::Node { rect: _, children: _, pixel_count } => *pixel_count,
        }
    }
}

pub struct BitImg {
    pixels: Pixels2D,
    layered_rect: LayeredRect,
}

impl BitImg {
    pub fn new(pixels: Pixels2D) -> Self {
        Self {
            layered_rect: LayeredRect::new(pixels.rect, &pixels),
            pixels,
        }
    }

    #[inline]
    pub fn is_opaque_at(&self, x: usize, y: usize) -> bool {
        self.pixels.pixel_at(x, y)
    }

    fn contains_pixel_in_layer(&self, target_rect: &Rect, layered: &LayeredRect) -> bool {
        match layered {
            LayeredRect::Leaf { rect, pixel_count } => {
                if *pixel_count == 0 { return false; }
                if ! rect.intersects(*target_rect) { return false; }
                if target_rect.contains_rect(*rect) && *pixel_count != 0 { return true; }
                self.pixels.contains_pixel(target_rect)
            },
            LayeredRect::Node { rect, children, pixel_count } => {
                if *pixel_count == 0 { return false; }
                if ! rect.intersects(*target_rect) { return false; }
                if target_rect.contains_rect(*rect) && *pixel_count != 0 { return true; }
                if self.contains_pixel_in_layer(target_rect, &children[0]) { return true; }
                if self.contains_pixel_in_layer(target_rect, &children[1]) { return true; }
                return false;
            },
        }
    }

    pub fn contains_pixel(&self, rect: &Rect) -> bool {
        let covered_both = rect.intersect(self.pixels.rect);
        if covered_both == Rect::NOTHING { return false; }

        self.contains_pixel_in_layer(&covered_both, &self.layered_rect)
    }
}

pub struct Img {
    texture: TextureHandle,
    bit_img: BitImg,
}

impl Img {
    pub fn from_img<T>(name: T, img: ColorImage, ctx: &Context) -> Self where T: Into<String> {
        let bits = to_bitset(&img);
        let texture = ctx.load_texture(name, img, TextureFilter::Linear);
        let size = texture.size();
        Self {
            bit_img: BitImg::new(Pixels2D::new(bits, Rect::from_min_size(Pos2::ZERO, Vec2::new(size[0] as f32, size[1] as f32)))),
            texture,
        }
    }

    #[inline]
    pub fn size(&self) -> Vec2 {
        self.texture.size_vec2()
    }

    #[inline]
    pub fn is_opaque_at(&self, x: usize, y: usize) -> bool {
        self.bit_img.is_opaque_at(x, y)
    }

    #[inline]
    pub fn contains_pixel(&self, rect: &Rect) -> bool {
        self.bit_img.contains_pixel(rect)
    }
}

pub enum SvgError {
    CannotLoad { width: u32, height: u32 },
    CannotRender,
}

pub fn load_svg_bytes(svg_bytes: &[u8], scale: f32) -> Result<egui::ColorImage, SvgError> {
    let mut opt = usvg::Options::default();
    opt.fontdb.load_system_fonts();

    let svg_tree = usvg::Tree::from_data(svg_bytes, &opt.to_ref()).map_err(|err| err.to_string())?;

    let pixmap_size = svg_tree.svg_node().size.to_screen_size();
    let [w, h] = [pixmap_size.width(), pixmap_size.height()];

    let mut pixmap = tiny_skia::Pixmap::new(w, h)
        .ok_or_else(|| SvgError::CannotLoad { width: w, height: h })?;

    resvg::render(
        &svg_tree,
        usvg::FitTo::Original,
        Default::default(),
        pixmap.as_mut(),
    ).ok_or_else(|| SvgError::CannotRender)?;

    let mut scaled_pixmap = tiny_skia::Pixmap::new(((w as f32) * scale) as u32, ((h as f32) * scale) as u32).unwrap();
    scaled_pixmap.draw_pixmap(0, 0, pixmap.as_ref(), &PixmapPaint::default(), Transform::from_scale(scale, scale), None);

    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [scaled_pixmap.width() as _, scaled_pixmap.height() as _],
        scaled_pixmap.data(),
    ))
}

pub fn to_bitset(img: &ColorImage) -> BitSet {
    let w = img.width();
    let h = img.height();
    let mut bitset = BitSet::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            if img[(x, y)] != Color32::TRANSPARENT {
                bitset.insert(w * y + x);
            }
        }
    }

    bitset
}

#[cfg(test)]
mod tests {
    use bit_set::BitSet;
    use egui::{ColorImage, Color32, Context, Rect, Pos2, Vec2};
    use crate::{Img, Pixels2D, LayeredRect};

    use super::to_bitset;

    const T: Color32 = Color32::TRANSPARENT;
    const B: Color32 = Color32::BLACK;

    #[test]
    fn img_can_convert_to_bitset() {
        let img = ColorImage {
            size: [8, 16],
            pixels: vec![
            //  0  1  2  3  4  5  6  7    
                T, T, T, T, T, T, T, T, // 0
                T, T, T, T, T, T, T, T, // 1
                T, T, B, T, T, T, B, B, // 2
                T, T, T, T, T, T, B, T, // 3
                T, T, T, T, T, T, T, T, // 4
                T, T, T, T, T, T, T, T, // 5
                T, B, B, T, T, T, T, T, // 6
                T, T, T, T, T, T, T, T, // 7
                T, T, T, T, T, T, T, T, // 8
                T, T, T, T, T, T, T, T, // 9
                T, T, T, T, T, T, T, T, // 10
                T, T, T, T, T, T, T, T, // 11
                T, T, T, T, T, T, T, B, // 12
                T, T, T, T, T, T, T, T, // 13
                B, B, T, T, T, T, T, T, // 14
                T, T, T, T, T, T, T, T, // 15
            ],
        };
        let bitset = to_bitset(&img);
        assert!(!bitset.contains(0));
        assert!(bitset.contains(18));
        assert!(bitset.contains(22));
        assert!(bitset.contains(23));
        assert!(bitset.contains(30));
        assert!(bitset.contains(49));
        assert!(bitset.contains(50));
    }

    #[test]
    fn is_opaque() {
        let img = ColorImage {
            size: [8, 16],
            pixels: vec![
            //  0  1  2  3  4  5  6  7    
                T, T, T, T, T, T, T, T, // 0
                T, T, T, T, T, T, T, T, // 1
                T, T, B, T, T, T, B, B, // 2
                T, T, T, T, T, T, B, T, // 3
                T, T, T, T, T, T, T, T, // 4
                T, T, T, T, T, T, T, T, // 5
                T, B, B, T, T, T, T, T, // 6
                T, T, T, T, T, T, T, T, // 7
                T, T, T, T, T, T, T, T, // 8
                T, T, T, T, T, T, T, T, // 9
                T, T, T, T, T, T, T, T, // 10
                T, T, T, T, T, T, T, T, // 11
                T, T, T, T, T, T, T, B, // 12
                T, T, T, T, T, T, T, T, // 13
                B, B, T, T, T, T, T, T, // 14
                T, T, T, T, T, T, T, T, // 15
            ],
        };

        let ctx = Context::default();
        let img = Img::from_img("test", img, &ctx);

        assert!(!img.is_opaque_at(0, 0));
        assert!(img.is_opaque_at(2, 2));
        assert!(img.is_opaque_at(6, 2));
        assert!(img.is_opaque_at(6, 2));
        assert!(img.is_opaque_at(0, 14));
        assert!(img.is_opaque_at(1, 14));
        assert!(!img.is_opaque_at(7, 15));
    }

    #[test]
    fn small_bitimg_becomes_leaf() {
        // O__
        // __O
        // OOO
        let mut bit_set = BitSet::with_capacity(9);
        bit_set.insert(0);
        bit_set.insert(5);
        bit_set.insert(6);
        bit_set.insert(7);
        bit_set.insert(8);
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(3., 3.));
        let bit_img = Pixels2D::new(bit_set, rect);
        let layered = LayeredRect::new(rect, &bit_img);
        assert_eq!(layered, LayeredRect::Leaf { rect, pixel_count: 5 });
    }

    #[test]
    fn bitimg_split_horizontal() {
        // O__O
        // __O_
        // OOO_
        let mut bit_set = BitSet::with_capacity(12);
        bit_set.insert(0);
        bit_set.insert(3);
        bit_set.insert(6);
        bit_set.insert(8);
        bit_set.insert(9);
        bit_set.insert(10);

        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(4., 3.));
        let bit_img = Pixels2D::new(bit_set, rect);
        let layered = LayeredRect::new(rect, &bit_img);
        let [left, right] = match layered {
            LayeredRect::Leaf { rect: _, pixel_count: _ } => { panic!("Test failed."); },
            LayeredRect::Node { rect: r, children, pixel_count } => {
                assert_eq!(rect, r);
                assert_eq!(pixel_count, 6);
                children
            },
        };

        // Left
        // O_
        // __
        // OO
        match *left {
            LayeredRect::Node { rect: _, children: _, pixel_count: _ } => { panic!("Test failed."); },
            LayeredRect::Leaf { rect, pixel_count } => {
                assert_eq!(pixel_count, 3);
                assert_eq!(rect, Rect::from_min_size(Pos2::ZERO, Vec2::new(2., 3.)));
            },
        };

        // Right
        // _O
        // O_
        // O_
        match *right {
            LayeredRect::Node { rect: _, children: _, pixel_count: _ } => { panic!("Test failed."); },
            LayeredRect::Leaf { rect, pixel_count } => {
                assert_eq!(pixel_count, 3);
                assert_eq!(rect, Rect::from_min_size(Pos2::new(2., 0.), Vec2::new(2., 3.)));
            },
        }
    }

    #[test]
    fn pixels2d_can_determine_rect_contains_pixels() {
        // O__
        // __O
        // OOO
        let mut bitset = BitSet::with_capacity(9);
        bitset.insert(0);
        bitset.insert(5);
        bitset.insert(6);
        bitset.insert(7);
        bitset.insert(8);

        let pixels = Pixels2D::new(bitset, Rect::from_min_size(Pos2::ZERO, Vec2::new(3., 3.)));
        assert!(pixels.contains_pixel(&Rect::from_min_size(Pos2::ZERO, Vec2::new(1.0, 1.0))));
        assert!(!pixels.contains_pixel(&Rect::from_min_size(Pos2::new(1.0, 0.0), Vec2::new(1.0, 1.0))));
        assert!(pixels.contains_pixel(&Rect::from_min_size(Pos2::ZERO, Vec2::new(1.0, 2.0))));
        assert!(!pixels.contains_pixel(&Rect::from_min_size(Pos2::new(1.0, 0.0), Vec2::new(1.0, 2.0))));
        assert!(pixels.contains_pixel(&Rect::from_min_size(Pos2::new(1.0, 0.0), Vec2::new(1.0, 3.0))));
        assert!(!pixels.contains_pixel(&Rect::from_min_size(Pos2::new(1.0, 0.0), Vec2::new(2.0, 1.0))));
        assert!(!pixels.contains_pixel(&Rect::from_min_size(Pos2::new(2.0, 0.0), Vec2::new(2.0, 1.0))));
        assert!(!pixels.contains_pixel(&Rect::from_min_size(Pos2::new(10.0, 10.0), Vec2::new(20.0, 20.0))));
    }

    #[test]
    fn bitimg_can_determine_rect_contains_pixels() {
        // O___O
        // __OO_
        // OOO__
        // _OO__
        // _____
        let mut bitset = BitSet::with_capacity(25);
        bitset.insert(0);
        bitset.insert(4);
        bitset.insert(7);
        bitset.insert(8);
        bitset.insert(10);
        bitset.insert(11);
        bitset.insert(12);
        bitset.insert(16);
        bitset.insert(17);

        let pixels = Pixels2D::new(bitset, Rect::from_min_size(Pos2::ZERO, Vec2::new(5., 5.)));
        assert!(pixels.contains_pixel(&Rect::from_min_size(Pos2::ZERO, Vec2::new(1.0, 1.0))));
        assert!(!pixels.contains_pixel(&Rect::from_min_size(Pos2::new(1.0, 0.0), Vec2::new(1.0, 1.0))));
        assert!(pixels.contains_pixel(&Rect::from_min_size(Pos2::ZERO, Vec2::new(1.0, 2.0))));
        assert!(!pixels.contains_pixel(&Rect::from_min_size(Pos2::new(1.0, 0.0), Vec2::new(1.0, 2.0))));
        assert!(pixels.contains_pixel(&Rect::from_min_size(Pos2::new(1.0, 0.0), Vec2::new(1.0, 3.0))));
        assert!(!pixels.contains_pixel(&Rect::from_min_size(Pos2::new(1.0, 0.0), Vec2::new(2.0, 1.0))));
        assert!(!pixels.contains_pixel(&Rect::from_min_size(Pos2::new(2.0, 0.0), Vec2::new(2.0, 1.0))));
        assert!(!pixels.contains_pixel(&Rect::from_min_size(Pos2::new(3.0, 2.0), Vec2::new(2.0, 2.0))));
        assert!(!pixels.contains_pixel(&Rect::from_min_size(Pos2::new(0.0, 4.0), Vec2::new(2.0, 2.0))));
        assert!(pixels.contains_pixel(&Rect::from_min_size(Pos2::new(0.0, 3.0), Vec2::new(3.0, 3.0))));
        assert!(!pixels.contains_pixel(&Rect::from_min_size(Pos2::new(10.0, 10.0), Vec2::new(20.0, 20.0))));
    }
}
