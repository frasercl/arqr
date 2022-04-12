
use std::{
    ops::{Deref, DerefMut},
    slice,
};
use image::{ImageBuffer, Pixel};

type U8Histo = [usize; 0x100];

fn img_to_u8_histo<Px, C>(img: &ImageBuffer<Px, C>) -> U8Histo
where
    Px: Pixel<Subpixel = u8>,
    C: Deref<Target = [u8]> + DerefMut,
{
    let mut histo = [0; 0x100];
    for px in img.pixels() {
        let val = px.to_luma().0[0];
        histo[val as usize] += 1;
    }
    histo
}

// From "A Simple and Efficient Image Pre-processing for QR Decoder" (Chen, Yang, & Zhang)
fn u8_histo_to_threshold(histo: U8Histo) -> u8 {
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

/// Discount ImageBuffer for bools
#[derive(Clone, Debug, Default)]
pub struct Bitmap {
    data: Vec<bool>,
    width: u32,
    height: u32,
}

impl Bitmap {
    pub fn from_u8_img_dynamic<Px, C>(img: &ImageBuffer<Px, C>) -> Self
    where
        Px: Pixel<Subpixel = u8>,
        C: Deref<Target = [u8]> + DerefMut,
    {
        // TODO: this takes two passes on the images
        // and converts to grayscale both times.
        // can it convert just once... and maybe even reuse the buffer?!
        let (width, height) = img.dimensions();
        let mut data = Vec::with_capacity((width * height) as usize);
        let thresh = u8_histo_to_threshold(img_to_u8_histo(img));
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

    pub fn rows(&self) -> Rows {
        Rows(self.data.chunks_exact(self.width as usize))
    }
}

impl Deref for Bitmap {
    type Target = [bool];
    fn deref(&self) -> &Self::Target {
        &*self.data
    }
}

pub struct Rows<'a>(slice::ChunksExact<'a, bool>);

impl<'a> Iterator for Rows<'a> {
    type Item = slice::Iter<'a, bool>;
    
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        Some(self.0.next()?.iter())
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
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
    fn next_back(&mut self) -> Option<Self::Item> {
        Some(self.0.next_back()?.iter())
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        Some(self.0.nth_back(n)?.iter())
    }
}
