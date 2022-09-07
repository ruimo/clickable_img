use std::mem::size_of;

use egui::{ColorImage, Color32};

const USIZE_BYTE_LEN: usize = size_of::<usize>();
const COLOR_BYTE_LEN: usize = Color32::BLACK.to_array().len();

pub fn img_to_u8(img: &ColorImage) -> Vec<u8> {
    let [width, height] = img.size;
    let mut vec = vec![0u8; USIZE_BYTE_LEN * 2 + width * height * COLOR_BYTE_LEN];
    let mut idx: usize = 0;

    vec[idx..(idx + USIZE_BYTE_LEN)].copy_from_slice(&width.to_ne_bytes());
    idx += USIZE_BYTE_LEN;
    vec[idx..(idx + USIZE_BYTE_LEN)].copy_from_slice(&height.to_ne_bytes());
    idx += USIZE_BYTE_LEN;

    for c in img.pixels.iter() {
        vec[idx..(idx + COLOR_BYTE_LEN)].copy_from_slice(&c.to_array());
        idx += COLOR_BYTE_LEN;
    }
    vec
}

pub fn u8_to_img(bin: &[u8]) -> ColorImage {
    let mut idx: usize = 0;
    let mut usize_buf = [0u8; USIZE_BYTE_LEN];
    usize_buf.copy_from_slice(&bin[idx..(idx + USIZE_BYTE_LEN)]);
    let width = usize::from_ne_bytes(usize_buf);
    idx += USIZE_BYTE_LEN;

    let mut usize_buf = [0u8; USIZE_BYTE_LEN];
    usize_buf.copy_from_slice(&bin[idx..(idx + USIZE_BYTE_LEN)]);
    let height = usize::from_ne_bytes(usize_buf);
    idx += USIZE_BYTE_LEN;

    let mut color_buf = [0u8; COLOR_BYTE_LEN];
    let pixel_count = width * height;
    let mut pixels: Vec<Color32> = Vec::with_capacity(pixel_count);
    for _ in 0..pixel_count {
        color_buf.copy_from_slice(&bin[idx..(idx + COLOR_BYTE_LEN)]);
        idx += COLOR_BYTE_LEN;
        pixels.push(Color32::from_rgba_premultiplied(color_buf[0], color_buf[1], color_buf[2], color_buf[3]));
    }

    ColorImage {
        size: [width, height],
        pixels: pixels,
    }
}

#[cfg(test)]
mod tests {
    use egui::{Color32, ColorImage};
    use super::{img_to_u8, u8_to_img};

    #[test]
    fn can_convert() {
        let width: usize = 3;
        let height: usize = 2;
        let pixels = vec![Color32::BLACK, Color32::RED, Color32::BLUE, Color32::WHITE, Color32::TRANSPARENT, Color32::GREEN];
        let img = ColorImage {
            size: [width, height],
            pixels: pixels.clone(),
        };

        let bin = img_to_u8(&img);
        let cvt_img = u8_to_img(&bin);

        assert_eq!(cvt_img.width(), width);
        assert_eq!(cvt_img.height(), height);
        assert_eq!(cvt_img.pixels, pixels);
    }
}