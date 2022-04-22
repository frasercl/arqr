//! Experimental image filters, written to be dropped into the image display
//! pipeline, not the decoding pipeline. As such, while they convert to
//! grayscale during the course of computation, they are generic over any image
//! type to save data and conversion overhead.

use std::ops::{Deref, DerefMut};
use image::{ImageBuffer, Pixel, Primitive};

#[inline]
fn abs_diff<Spx: Primitive>(a: Spx, b: Spx) -> Spx {
    if a < b { b - a } else { a - b }
}

// semi-hack to prevent overflow while keeping values generic
#[inline]
fn add_clamped<Spx: Primitive>(a: Spx, b: Spx) -> Spx {
    if Spx::DEFAULT_MAX_VALUE - a < b {
        Spx::DEFAULT_MAX_VALUE
    } else {
        a + b
    }
}

#[inline]
fn binarize_val<Spx: Primitive>(val: Spx, thresh: Spx) -> Spx {
    if val > thresh {
        Spx::DEFAULT_MAX_VALUE
    } else {
        Spx::DEFAULT_MIN_VALUE
    }
}

/// Highlight vertical edges
pub fn edge_v_in_place<Px, C>(img: &mut ImageBuffer<Px, C>)
where
    Px: Pixel,
    C: Deref<Target = [Px::Subpixel]> + DerefMut,
{
    for row in img.rows_mut() {
        let mut last = Px::Subpixel::DEFAULT_MIN_VALUE;
        for px in row {
            let val = px.to_luma().0[0];

            let diff = abs_diff(val, last);
            last = val;

            px.apply_without_alpha(|_| diff);
        }
    }
}

/// Highlight vertical edges and binarize
pub fn edge_v_binarized_in_place<Px, C>(img: &mut ImageBuffer<Px, C>, thresh: Px::Subpixel)
where
    Px: Pixel,
    C: Deref<Target = [Px::Subpixel]> + DerefMut,
{
    for row in img.rows_mut() {
        let mut last = Px::Subpixel::DEFAULT_MIN_VALUE;
        for px in row {
            let val = px.to_luma().0[0];

            let diff = binarize_val(abs_diff(val, last), thresh);
            last = val;

            px.apply_without_alpha(|_| diff);
        }
    }
}

/// Highlight horizontal edges
pub fn edge_h_in_place<Px, C>(img: &mut ImageBuffer<Px, C>)
where
    Px: Pixel,
    C: Deref<Target = [Px::Subpixel]> + DerefMut,
{
    let mut luma_buf = Vec::with_capacity(img.width() as usize);
    let mut row_iter = img.rows_mut();

    // Iterate first row only, convert to luma, save converted values
    for px in row_iter.next().unwrap() {
        let luma = px.to_luma().0[0];
        luma_buf.push(luma);
        px.apply_without_alpha(|_| luma);
    }

    for row in row_iter {
        for (px, last) in row.zip(luma_buf.iter_mut()) {
            let val = px.to_luma().0[0];

            let diff = abs_diff(val, *last);
            *last = val;

            px.apply_without_alpha(|_| diff);
        }
    }
}

/// Highlight horizontal and vertical edges, without considering corners
pub fn edge_2_in_place<Px, C>(img: &mut ImageBuffer<Px, C>)
where
    Px: Pixel,
    C: Deref<Target = [Px::Subpixel]> + DerefMut,
{
    let mut row_buf = vec![Px::Subpixel::DEFAULT_MIN_VALUE; img.width() as usize];

    for row in img.rows_mut() {
        let mut last_x = Px::Subpixel::DEFAULT_MIN_VALUE;
        for (px, last_y) in row.zip(row_buf.iter_mut()) {
            let val = px.to_luma().0[0];
            let diff = add_clamped(abs_diff(val, last_x), abs_diff(val, *last_y));

            last_x = val;
            *last_y = val;

            px.apply_without_alpha(|_| diff);
        }
    }
}

/// Highlight horizontal and vertical edges; consider corners
pub fn edge_3_in_place<Px, C>(img: &mut ImageBuffer<Px, C>)
where
    Px: Pixel,
    C: Deref<Target = [Px::Subpixel]> + DerefMut,
{
    let mut row_buf = vec![Px::Subpixel::DEFAULT_MIN_VALUE; img.width() as usize];

    for row in img.rows_mut() {
        let mut last_x = Px::Subpixel::DEFAULT_MIN_VALUE;
        let mut last_xy = Px::Subpixel::DEFAULT_MIN_VALUE;
        for (px, last_y) in row.zip(row_buf.iter_mut()) {
            let val = px.to_luma().0[0];
            let diff = add_clamped(
                add_clamped(abs_diff(val, last_x), abs_diff(val, *last_y)),
                abs_diff(val, last_xy)
            );

            last_xy = *last_y;
            *last_y = val;
            last_x = val;

            px.apply_without_alpha(|_| diff);
        }
    }
}



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

pub fn binarize_u8_in_place<Px, C>(img: &mut ImageBuffer<Px, C>)
where
    Px: Pixel<Subpixel = u8>,
    C: Deref<Target = [u8]> + DerefMut,
{
    let thresh = u8_histo_to_threshold(img_to_u8_histo(img));

    for px in img.pixels_mut() {
        let luma = binarize_val(px.to_luma().0[0], thresh);
        px.apply_without_alpha(|_| luma);
    }
}
