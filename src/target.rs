use std::{iter, slice};
use crate::bitmap::Bitmap;

#[derive(Clone, Copy, Debug, Default)]
pub struct Target {
    pub x_min: usize,
    pub y_min: usize,
    pub x_max: usize,
    pub y_max: usize,
}

// A dead simple fixed-length circular buffer
// Useful for spotting patterns in lines of pixels
// Write with push, read with iter or peek_back
#[derive(Clone, Copy, Debug)]
struct FixedBuffer<T: Copy + Default, const N: usize> {
    data: [T; N],
    head: usize,
    full: bool,
}

type Iter<'a, T> = iter::Chain<slice::Iter<'a, T>, slice::Iter<'a, T>>;

impl<T: Copy + Default, const N: usize> FixedBuffer<T, N> {
    pub fn new() -> Self {
        Self { data: [T::default(); N], head: 0, full: false }
    }

    pub fn is_full(&self) -> bool {
        self.full
    }

    // Allow one buffer to be reused for many scans
    pub fn clear(&mut self) {
        self.head = 0;
        self.full = false;
    }

    pub fn push(&mut self, val: T) {
        self.data[self.head] = val;
        self.head = (self.head + 1) % N;
        if self.head == 0 && !self.full {
            self.full = true;
        }
    }

    pub fn peek_back(&self) -> T {
        self.data[if self.full { self.head } else { 0 }]
    }

    pub fn iter(&self) -> Iter<T> {
        let start = if self.full { self.head } else { N };
        self.data[start..N].iter().chain(self.data[0..self.head].iter())
    }
}

const TARGET_RATIOS: [f32; 4] = [1.0, 1.0/3.0, 3.0, 1.0];
const TARGET_THRESH: f32 = 0.5;

#[inline]
fn confirm_pos_target(img: &Bitmap, x_min: usize, x_max: usize, y: usize) -> Option<Target> {
    let target_width = x_max - x_min;
    let center_x = x_min + target_width / 2;
    let img_width = img.width() as usize;
    let point_idx = y * img_width + center_x;
    let max = target_width * img_width;

    let mut size_buf = [0; 5];
    let mut size_idx = 2;
    let mut color = false;
    let mut y_min = y;

    // Iterate up, count the size of black & white chunks
    let max_up = if max > point_idx {
        center_x
    } else {
        point_idx - max
    };
    for &px in img[max_up..point_idx].iter().rev().step_by(img_width) {
        if px != color {
            color = px;
            if size_idx == 0 { break; }
            size_idx -= 1;
        }
        size_buf[size_idx] += 1;
        y_min -= 1;
    }

    size_idx = 2;
    color = false;
    let mut y_max = y;

    // Iterate down, count the size of black and white chunks
    let max_down = if max > img.len() - point_idx {
        img.len() - (img_width - center_x)
    } else {
        point_idx + max
    };
    for &px in img[point_idx..max_down].iter().step_by(img_width) {
        if px != color {
            color = px;
            if size_idx == 4 { break; }
            size_idx += 1;
        }
        size_buf[size_idx] += 1;
        y_max += 1;
    }

    // fun with iterators (calculate chunk size ratios & confirm they match)
    let is_pattern = size_buf
        .windows(2)
        .map(|win| win[0] as f32 / win[1] as f32)
        .zip(TARGET_RATIOS.iter())
        .all(|(ratio, target)| {
            let off_by = ratio - target;
            -TARGET_THRESH < off_by && off_by < TARGET_THRESH
        });

    if is_pattern {
        Some(Target{x_min, y_min, x_max, y_max})
    } else { None }
}

pub fn find_pos_targets(img: &Bitmap) -> Vec<Target> {
    // Stores the ratios of sizes of successive chunks of pixels
    let mut ratio_buf = FixedBuffer::<f32, 4>::new();
    // Stores the x-coords of the last few chunk edges
    let mut x_buf = FixedBuffer::<usize, 6>::new();
    // Holds any targets we find
    let mut targets = Vec::new();
    // Tracks targets that are in danger of being re-scanned
    let mut active_targets = Vec::new();

    for (y, row) in img.rows().enumerate().step_by(4) {
        let mut enum_row = row.enumerate();
        let (_, &(mut chunk_color)) = enum_row.next().unwrap();
        let mut last_count = 1;
        // advance through the first chunk and save its size in last_count
        while let Some((_, &px)) = enum_row.next() {
            if px != chunk_color { break; }
            last_count += 1;
        }
        let mut count = 1; // count size of current chunk of black/white

        x_buf.push(last_count);
        chunk_color = !chunk_color;

        for (x, &px) in enum_row {
            if px != chunk_color {
                chunk_color = px;

                x_buf.push(x);
                ratio_buf.push(last_count as f32 / count as f32);

                last_count = count;
                count = 1;
                // check that we've just moved from black to white
                // and have enough chunks for a pattern
                if !chunk_color || !ratio_buf.is_full() {
                    continue;
                }

                // check that this pattern isn't within any active targets
                let start_x = x_buf.peek_back();
                let outside = |&i| {
                    let t: Target = targets[i];
                    x < t.x_min || start_x > t.x_max
                };
                if !active_targets.iter().all(outside) {
                    continue;
                }

                // now test if this pattern matches the shape of a target
                let is_pattern = ratio_buf
                    .iter()
                    .zip(TARGET_RATIOS.iter())
                    .all(|(ratio, target)| {
                        let off_by = ratio - target;
                        -TARGET_THRESH < off_by && off_by < TARGET_THRESH
                    });
                if !is_pattern {
                    continue;
                }

                if let Some(target) = confirm_pos_target(img, start_x, x, y) {
                    active_targets.push(targets.len());
                    targets.push(target);
                }
                
            } else {
                count += 1;
            }
        }

        // clear out any active targets that we're now entirely below
        let mut ati = 0;
        while ati < active_targets.len() {
            if y > targets[active_targets[ati]].y_max {
                active_targets.swap_remove(ati);
            } else {
                ati += 1;
            }
        }

        ratio_buf.clear();
        x_buf.clear();
    }
    targets
}
