//! Contains the primary image struct, `Bitmap`, and basic image processing.
//! 
//! `Bitmap` is based on `image::ImageBuffer`, but has `bool`s for pixels.

// Quite a lot of this file just reimplements `ImageBuffer` and
// `slice` iterators. Oh well - it's a good exercise to do!

use std::{ops::Deref, slice, cmp};
use image::{ImageBuffer, Pixel, Primitive, buffer::ConvertBuffer};

type U8Histo = [usize; 0x100];

/// Creates a luminosity histogram from an image
fn img_to_u8_histo<Px, C>(img: &ImageBuffer<Px, C>) -> U8Histo
where
    Px: Pixel<Subpixel = u8>,
    C: Deref<Target = [u8]>,
{
    let mut histo = [0; 0x100];
    for px in img.pixels() {
        let val = px.to_luma().0[0];
        histo[val as usize] += 1;
    }
    histo
}

/// Given a luminosity histogram, picks a suitable binarization threshold.
/// 
/// Algorithm from "A Simple and Efficient Image Pre-processing for QR Decoder"
/// (Chen, Yang, & Zhang)
fn u8_histo_to_threshold(histo: &U8Histo) -> u8 {
    let mut thresh: usize = 0x80;

    let accum = |(sum, cnt), (hval, luma)| (sum + hval * luma, cnt + hval);
    let (mut black_sum, mut black_cnt) = histo[0x00..0x80].iter()
        .zip(0..)
        .fold((0, 1), accum); // Count starts at 1 to avoid dividing by 0
    let (mut white_sum, mut white_cnt) = histo[0x80..0x100].iter()
        .zip(0x80..)
        .fold((0, 1), accum);
    let mut new_thresh = (black_sum / black_cnt + white_sum / white_cnt) / 2;

    while new_thresh != thresh {
        let less = new_thresh < thresh;
        let (min, max) = if less {
            (new_thresh, thresh)
        } else {
            (thresh, new_thresh)
        };

        let (diff_sum, diff_cnt) = histo[min..max].iter()
            .zip(min..)
            .fold((0, 0), accum);
        
        if less {
            black_sum -= diff_sum;
            black_cnt -= diff_cnt;
            white_sum += diff_sum;
            white_cnt += diff_cnt;
        } else {
            black_sum += diff_sum;
            black_cnt += diff_cnt;
            white_sum -= diff_sum;
            white_cnt -= diff_cnt;
        }

        thresh = new_thresh;
        new_thresh = (black_sum / black_cnt + white_sum / white_cnt) / 2;
    }

    thresh as u8
}

/// Discount ImageBuffer with `bool`s for pixels
#[derive(Clone, Debug, Default)]
pub struct Bitmap {
    data: Vec<bool>,
    width: u32,
    height: u32,
}

impl Bitmap {
    /// Creates a new all white bitmap with the given width and height.
    pub fn new(width: u32, height: u32) -> Self {
        let data = vec![true; (width * height) as usize];
        Self { data, width, height }
    }

    /// Converts an `ImageBuffer` to `Bitmap` by dynamically picking a suitable
    /// binarization threshold
    pub fn from_u8_img_dynamic<Px, C>(img: &ImageBuffer<Px, C>) -> Self
    where
        Px: Pixel<Subpixel = u8>,
        C: Deref<Target = [u8]>,
    {
        // TODO: this takes two passes on the images
        // and converts to grayscale both times.
        // can it convert just once... and maybe even reuse the buffer?!
        let (width, height) = img.dimensions();
        let mut data = Vec::with_capacity((width * height) as usize);
        let thresh = u8_histo_to_threshold(&img_to_u8_histo(img));
        for px in img.pixels() {
            let luma = px.to_luma().0[0];
            data.push(luma > thresh);
        }

        Self { data, width, height }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn pixel_index_unchecked(&self, x: u32, y: u32) -> usize {
        (y * self.width + x) as usize
    }

    fn pixel_index(&self, x: u32, y: u32) -> Option<usize> {
        if x >= self.width || y >= self.height {
            return None
        }

        Some(self.pixel_index_unchecked(x, y))
    }

    pub fn get_pixel(&self, x: u32, y: u32) -> &bool {
        match self.pixel_index(x, y) {
            None => panic!(
                "Bitmap index {:?} out of bounds {:?}",
                (x, y),
                (self.width, self.height)
            ),
            Some(i) => &self.data[i],
        }
    }

    pub fn get_pixel_mut(&mut self, x: u32, y: u32) -> &mut bool {
        match self.pixel_index(x, y) {
            None => panic!(
                "Bitmap index {:?} out of bounds {:?}",
                (x, y),
                (self.width, self.height)
            ),
            Some(i) => &mut self.data[i],
        }
    }

    pub fn get_pixel_checked(&self, x: u32, y: u32) -> Option<&bool> {
        Some(&self.data[self.pixel_index(x, y)?])
    }

    pub fn get_pixel_checked_mut(&mut self, x: u32, y: u32) -> Option<&mut bool> {
        let i = self.pixel_index(x, y)?;
        Some(&mut self.data[i])
    }

    fn clamp_coords(&self, x: u32, y: u32) -> (u32, u32) {
        let cx = cmp::min(x, self.width - 1);
        let cy = cmp::min(y, self.height - 1);
        (cx, cy)
    }

    pub fn get_pixel_clamped(&self, x: u32, y: u32) -> &bool {
        let (cx, cy) = self.clamp_coords(x, y);
        self.get_pixel(cx, cy)
    }

    pub fn get_pixel_clamped_mut(&mut self, x: u32, y: u32) -> &mut bool {
        let (cx, cy) = self.clamp_coords(x, y);
        self.get_pixel_mut(cx, cy)
    }

    /// Returns an iterator over the rows of pixels in this bitmap
    pub fn rows(&self) -> Rows {
        Rows(self.data.chunks_exact(self.width as usize))
    }

    /// Returns an iterator over the mutable rows of this bitmap
    pub fn rows_mut(&mut self) -> RowsMut {
        RowsMut(self.data.chunks_exact_mut(self.width as usize))
    }
}

impl Deref for Bitmap {
    type Target = [bool];
    fn deref(&self) -> &Self::Target {
        &*self.data
    }
}

impl<Px: Pixel> ConvertBuffer<ImageBuffer<Px, Vec<Px::Subpixel>>> for Bitmap {
    fn convert(&self) -> ImageBuffer<Px, Vec<Px::Subpixel>> {
        let mut buffer: ImageBuffer<Px, Vec<Px::Subpixel>> =
            ImageBuffer::new(self.width, self.height);
        for (px, &bit) in buffer.pixels_mut().zip(self.iter()) {
            let val = if bit {
                Px::Subpixel::DEFAULT_MAX_VALUE
            } else {
                Px::Subpixel::DEFAULT_MIN_VALUE
            };
            px.apply_with_alpha(|_| val, |_| Px::Subpixel::DEFAULT_MAX_VALUE);
        }
        buffer
    }
}

/// Iterator over rows of pixels in a bitmap
pub struct Rows<'a>(slice::ChunksExact<'a, bool>);

impl<'a> Iterator for Rows<'a> {
    type Item = slice::Iter<'a, bool>;
    
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        Some(self.0.next()?.iter())
    }

    #[inline]
    fn count(self) -> usize {
        self.0.count()
    }

    #[inline]
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        Some(self.0.nth(n)?.iter())
    }

    #[inline]
    fn last(mut self) -> Option<Self::Item> {
        self.next_back()
    }
}

impl DoubleEndedIterator for Rows<'_> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        Some(self.0.next_back()?.iter())
    }

    #[inline]
    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        Some(self.0.nth_back(n)?.iter())
    }
}

/// Iterator over mutable rows of pixels in a bitmap
pub struct RowsMut<'a>(slice::ChunksExactMut<'a, bool>);

impl<'a> Iterator for RowsMut<'a> {
    type Item = slice::IterMut<'a, bool>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        Some(self.0.next()?.iter_mut())
    }

    #[inline]
    fn count(self) -> usize {
        self.0.count()
    }

    #[inline]
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        Some(self.0.nth(n)?.iter_mut())
    }

    #[inline]
    fn last(mut self) -> Option<Self::Item> {
        self.next_back()
    }
}

impl DoubleEndedIterator for RowsMut<'_> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        Some(self.0.next_back()?.iter_mut())
    }

    #[inline]
    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        Some(self.0.nth_back(n)?.iter_mut())
    }
}

pub fn affine_transform_chunk(
    source: &Bitmap,
    trans: [[f64; 3]; 2],
    width: u32,
    height: u32,
) -> Bitmap {
    let mut result = Bitmap::new(width, height);
    // We want to "pick" result's pixels from source, not map source to result.
    // Therefore, we first invert the matrix.
    let [[a, b, tx], [c, d, ty]] = trans;
    let det = a * d - b * c;
    let [[ap, bp], [cp, dp]] = [[d / det, -b / det], [-c / det, a / det]];
    println!("{:?}", [[ap, bp, -tx], [cp, dp, -ty]]);

    for (y, row) in result.rows_mut().enumerate() {
        let y = y as f64;
        for (x, px) in row.enumerate() {
            let x = x as f64;
            let sx = ((a * x + c * y) + tx) as u32;
            let sy = ((b * x + d * y) + ty) as u32;
            // let sx = (x - tx) as u32;
            // let sy = (y - ty) as u32;
            *px = *source.get_pixel_checked(sx, sy).unwrap_or(&true);
        }
    }

    result
}
