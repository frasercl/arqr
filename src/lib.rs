
use std::{ops::Deref, f64::consts::PI};
use image::{ImageBuffer, Rgba, buffer::ConvertBuffer, Pixel};

pub mod bitmap;
pub mod target;
pub mod filter;

use target::{
    find_pos_targets,
    pick_corners,
    to_side_len,
    to_affine_transform,
};
use bitmap::{Bitmap, affine_transform_chunk};

#[derive(Clone, Copy, Debug, Default)]
pub struct Point<T> { pub x: T, pub y: T }

impl<T> From<(T, T)> for Point<T> {
    fn from(tup: (T, T)) -> Self {
        Self { x: tup.0, y: tup.1 }
    }
}

impl<T: Copy> From<[T; 2]> for Point<T> {
    fn from(arr: [T; 2]) -> Self {
        Self { x: arr[0], y: arr[1] }
    }
}

impl<T> Point<T> {
    pub fn new(x: T, y: T) -> Self {
        Self { x, y }
    }
}

impl<T: Into<f64>> Point<T> {
    pub fn to_f64(self) -> Point<f64> {
        Point { x: self.x.into(), y: self.y.into() }
    }
}

impl Point<f64> {
    pub fn dist_to(&self, other: Point<f64>) -> f64 {
        ((other.x - self.x).powi(2) + (other.y - self.y).powi(2)).sqrt()
    }

    pub fn angle_to(&self, other: Point<f64>) -> f64 {
        let (x_diff, y_diff) = (other.x - self.x, other.y - self.y);
        let arc = (y_diff / x_diff).atan();
        if x_diff < 0.0 {
            if y_diff < 0.0 {
                arc - PI
            } else {
                arc + PI
            }
        } else {
            arc
        }
    }
}

#[derive(Debug, Default)]
pub struct ScanResult {
    pub targets: Vec<target::Target<f64>>,
    pub bbox: Option<[Point<f64>; 3]>,
    pub code_img: Option<ImageBuffer<Rgba<u8>, Vec<u8>>>,
    pub vectors: Option<[Point<f64>; 2]>,
}

impl ScanResult {
    pub fn new() -> Self {
        Self { targets: Vec::new(), ..Default::default() }
    }
}

pub fn scan<Px, C>(img: &ImageBuffer<Px, C>) -> ScanResult
where
    Px: Pixel<Subpixel = u8>,
    C: Deref<Target = [u8]>
{
    let bmp = Bitmap::from_u8_img_dynamic(img);
    let targets = find_pos_targets(&bmp);
    let bbox = pick_corners(&targets);
    let mut vectors = None;
    let code_img = if let Some(bbox) = bbox {
        let len = to_side_len(bbox);
        let trans = to_affine_transform(bbox, len);
        // println!("{:?}", trans);
        let width = img.width() / 2;
        let angle_h = bbox[0].angle_to(bbox[1]);
        let angle_v = bbox[0].angle_to(bbox[1]);
        let vector_h = Point::new(200.0 * angle_h.cos(), 200.0 * angle_h.sin());
        let vector_v = Point::new(200.0 * angle_v.cos(), 200.0 * angle_v.sin());
        vectors = Some([vector_h, vector_v]);
        Some(affine_transform_chunk(&bmp, trans, width, width).convert())
    } else { None };
    let targets = targets.into_iter().map(|t| t.to_f64()).collect();
    ScanResult { targets, bbox, code_img, vectors }
}
