use std::path::Path;
use std::hash::{Hash, Hasher};

use bit_set::BitSet;
use egui::{ColorImage, Color32, TextureHandle, Vec2, Context, Rect, Pos2, TextureId, TextureOptions};
use img_converter::{img_to_u8, u8_to_img};
use local_file_cache::LocalFileCache;
use sha::sha256::Sha256;

pub mod img_converter;

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
    
    pub fn dump(&self) {
        for y in 0..(self.rect.height() as usize) {
            for x in 0..(self.rect.width() as usize) {
                print!("{}", if self.pixel_at(x, y) { "X" } else {" "});
            }
            println!("");
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
                if self.pixel_at(x, y) { 
                    count += 1;
                }
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
    let left_rect = Rect::from_min_size(rect.left_top(), Vec2::new(split_at as f32, rect.height()));
    let right_rect = Rect::from_min_size(
        Pos2::new(rect.left() + split_at as f32, rect.top()),
        Vec2::new(rect.width() - split_at as f32, rect.height())
    );

    [ left_rect, right_rect ]
}

pub fn split_vertical(rect: &Rect) -> [Rect; 2] {
    let split_at = (rect.height() as usize) / 2;
    let top_rect = Rect::from_min_size(rect.left_top(), Vec2::new(rect.width(), split_at as f32));
    let bottom_rect = Rect::from_min_size(
        Pos2::new(rect.left(), rect.top() + split_at as f32),
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
            let cnt = bit_img.pixel_count(rect);
            LayeredRect::Leaf {
                rect,
                pixel_count: cnt,
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
    
    pub fn dump(&self) {
        self.pixels.dump();
        println!("layered_rect: {:?}", self.layered_rect);
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
        let texture = ctx.load_texture(name, img, TextureOptions::LINEAR);
        let size = texture.size();
        let pixels = Pixels2D::new(bits, Rect::from_min_size(Pos2::ZERO, Vec2::new(size[0] as f32, size[1] as f32)));
        let bit_img = BitImg::new(pixels);
        Self {
            bit_img, texture,
        }
    }

    pub fn from_svg<T>(name: T, svg_bytes: &[u8], scale: f32, ctx: &Context) -> Result<Self, SvgError> where T: Into<String> {
        let img = load_svg_bytes(svg_bytes, scale)?;
        Ok(Self::from_img(name, img, ctx))
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

    #[inline]
    pub fn texture_id(&self) -> TextureId {
        self.texture.id()
    }
}

#[derive(Debug)]
pub enum SvgError {
    CannotParse(usvg::Error),
    CannotLoad { width: u32, height: u32 },
    CannotRender,
    Other(String),
}

pub struct SvgLoader {
    pub scale: f32,
    pub cache: Option<LocalFileCache<Result<ColorImage, SvgError>>>,
}

impl SvgLoader {
    pub fn new<P>(scale: f32, cache_dir: Option<P>) -> Self where P: AsRef<Path> {
        Self {
            scale,
            cache: cache_dir.and_then(|p| LocalFileCache::<Result<ColorImage, SvgError>>::new(p,
                Box::new(|img|
                    match img {
                        Ok(ci) => Some(img_to_u8(ci)),
                        Err(_) => None,
                    }
                ),
                Box::new(|bin| Ok(u8_to_img(bin)))
            )),
        }
    }

    pub fn load(&self, svg_bytes: &[u8]) -> Result<egui::ColorImage, SvgError> {
        match self.cache.as_ref() {
            Some(cache) => {
                let mut hash = Sha256::default();
                <u8 as Hash>::hash_slice(&self.scale.to_ne_bytes(), &mut hash);
                <u8 as Hash>::hash_slice(svg_bytes, &mut hash);
                let hex_str = format!("{:x}", hash.finish());
                let fname = Path::new(&hex_str);
                match cache.or_insert_with(fname, || load_svg_bytes(svg_bytes, self.scale)) {
                    Ok(ok) => ok,
                    Err(io_err) => Err(SvgError::Other(io_err.to_string()))
                }
            },
            None => load_svg_bytes(svg_bytes, self.scale),
        }
    }
}

pub fn load_svg_bytes(svg_bytes: &[u8], scale: f32) -> Result<egui::ColorImage, SvgError> {
    let opt = usvg::Options::default();
    let usvg_tree: usvg::Tree = usvg::Tree::from_data(svg_bytes, &opt).map_err(|err: usvg::Error| SvgError::CannotParse(err))?;
    let size = usvg_tree.size();
    let w = size.width().ceil() as usize;
    let h = size.height().ceil() as usize;

    let mut pixmap = resvg::tiny_skia::Pixmap::new(((w as f32) * scale) as u32, ((h as f32) * scale) as u32)
        .ok_or_else(|| SvgError::CannotLoad { width: w as u32, height: h as u32})?;
    resvg::render(&usvg_tree, usvg::Transform::from_scale(scale, scale), &mut pixmap.as_mut());

    let img = egui::ColorImage::from_rgba_unmultiplied(
        [pixmap.width() as usize, pixmap.height() as usize], pixmap.data(),
    );

    Ok(img)
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
    use std::io::ErrorKind;
    use bit_set::BitSet;
    use egui::{ColorImage, Color32, Context, Rect, Pos2, Vec2};
    use local_file_cache::LocalFileCache;
    use crate::{Img, Pixels2D, LayeredRect, load_svg_bytes, SvgLoader};

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
    
    const TEST_SVG: &'static [u8] = br#"<?xml version="1.0" standalone="no"?>
    <!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd" >
    <svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" version="1.1" viewBox="0 0 100 100">
       <rect x="0" y="0" width="99" height="99" style="fill:rgb(255,0,0);stroke-width:1"/>
    </svg>
    "#;
    
    #[test]
    fn img_contains_pixel() {
        let img: ColorImage = load_svg_bytes(TEST_SVG, 1.0).unwrap();
        let ctx = Context::default();
        let img: Img = Img::from_img("test", img, &ctx);
        assert!(!img.contains_pixel(&Rect::from_min_size(Pos2::new(99., 0.), Vec2::new(1., 1.))));
        assert!(img.contains_pixel(&Rect::from_min_size(Pos2::new(0., 0.), Vec2::new(1., 1.))));
    }

    #[test]
    fn svg_to_img() {
        let img: ColorImage = load_svg_bytes(TEST_SVG, 1.0).unwrap();
        assert_eq!(img.width(), 100);
        assert_eq!(img.height(), 100);
        assert_eq!(img[(0, 0)], Color32::RED);
        assert_eq!(img[(98, 0)], Color32::RED);
        assert_eq!(img[(99, 0)], Color32::TRANSPARENT);

        assert_eq!(img[(0, 98)], Color32::RED);
        assert_eq!(img[(0, 99)], Color32::TRANSPARENT);

        assert_eq!(img[(98, 98)], Color32::RED);
        assert_eq!(img[(99, 99)], Color32::TRANSPARENT);
    }

    #[test]
    fn can_cache() {
        let img: ColorImage = load_svg_bytes(TEST_SVG, 0.1).unwrap();
        if let Err(e) = LocalFileCache::<()>::invalidate("my_test").unwrap() {
            if e.kind() != ErrorKind::NotFound {
                panic!("Unexpected error {:?}", e);
            }
        }
        let loader = SvgLoader::new(0.1, Some("my_test"));
        let cached = loader.load(TEST_SVG).unwrap();

        assert_eq!(img.size, cached.size);
        assert_eq!(img.pixels, cached.pixels);

        let cached = loader.load(TEST_SVG).unwrap();
        assert_eq!(img.size, cached.size);
        assert_eq!(img.pixels, cached.pixels);

        for x in 0..cached.width() {
            for y in 0..cached.height() {
                assert_eq!(cached[(x, y)], Color32::RED);
            }
        }
    }

    #[test]
    fn do_split_horizontal() {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(10.0, 5.0));
        let [left, right] = crate::split_horizontal(&rect);
        assert_eq!(left, Rect::from_min_size(Pos2::ZERO, Vec2::new(5.0, 5.0)));
        assert_eq!(right, Rect::from_min_size(Pos2::new(5.0, 0.0), Vec2::new(5.0, 5.0)));

        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(5.0, 3.0));
        let [left, right] = crate::split_horizontal(&rect);
        assert_eq!(left, Rect::from_min_size(Pos2::ZERO, Vec2::new(2.0, 3.0)));
        assert_eq!(right, Rect::from_min_size(Pos2::new(2.0, 0.0), Vec2::new(3.0, 3.0)));

        let rect = Rect::from_min_size(Pos2::new(100.0, 200.0), Vec2::new(10.0, 5.0));
        let [left, right] = crate::split_horizontal(&rect);
        assert_eq!(left, Rect::from_min_size(Pos2::new(100.0, 200.0), Vec2::new(5.0, 5.0)));
        assert_eq!(right, Rect::from_min_size(Pos2::new(105.0, 200.0), Vec2::new(5.0, 5.0)));
    }

    #[test]
    fn do_split_vertical() {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(10.0, 20.0));
        let [left, right] = crate::split_vertical(&rect);
        assert_eq!(left, Rect::from_min_size(Pos2::ZERO, Vec2::new(10.0, 10.0)));
        assert_eq!(right, Rect::from_min_size(Pos2::new(0.0, 10.0), Vec2::new(10.0, 10.0)));

        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(10.0, 11.0));
        let [left, right] = crate::split_vertical(&rect);
        assert_eq!(left, Rect::from_min_size(Pos2::ZERO, Vec2::new(10.0, 5.0)));
        assert_eq!(right, Rect::from_min_size(Pos2::new(0.0, 5.0), Vec2::new(10.0, 6.0)));

        let rect = Rect::from_min_size(Pos2::new(100.0, 200.0), Vec2::new(10.0, 5.0));
        let [left, right] = crate::split_vertical(&rect);
        assert_eq!(left, Rect::from_min_size(Pos2::new(100.0, 200.0), Vec2::new(10.0, 2.0)));
        assert_eq!(right, Rect::from_min_size(Pos2::new(100.0, 202.0), Vec2::new(10.0, 3.0)));
    }
}
