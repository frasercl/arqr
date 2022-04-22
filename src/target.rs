//! Contains functions to locate position targets within the image, and to
//! locate the code as much as possible based on the positions of those targets.

use std::{iter, slice, f64::consts::{PI, TAU}};
use crate::{Point, bitmap::Bitmap};

/// Represents the location of a single identified position target.
/// 
/// Note that this is an *axis-aligned* box which does not represent the tilt
/// or skew of the target (the scanner hasn't identified those features of the
/// code at this stage). The only guarantee this box makes is that the
/// edges of the target pass through the midpoints of the box's borders, e.g.
/// the point `(min.x, mid.y)` is a point on the top edge of the target. The
/// `mid` field is provided to make finding these midpoints nicer.
#[derive(Clone, Copy, Debug, Default)]
pub struct Target<T: Copy> {
    pub min: Point<T>,
    pub mid: Point<T>,
    pub max: Point<T>,
}

impl<T: Copy> Target<T> {
    pub fn new(x_min: T, y_min: T, x_mid: T, y_mid: T, x_max: T, y_max: T) -> Self {
        Self {
            min: Point::new(x_min, y_min),
            mid: Point::new(x_mid, y_mid),
            max: Point::new(x_max, y_max),
        }
    }

    pub fn up(&self) -> Point<T> {
        Point::new(self.mid.x, self.min.y)
    }

    pub fn down(&self) -> Point<T> {
        Point::new(self.mid.x, self.max.y)
    }

    pub fn left(&self) -> Point<T> {
        Point::new(self.min.x, self.mid.y)
    }

    pub fn right(&self) -> Point<T> {
        Point::new(self.max.x, self.mid.y)
    }
}

impl<T: Copy + Into<f64>> Target<T> {
    pub fn to_f64(self) -> Target<f64> {
        Target {
            min: self.min.to_f64(),
            mid: self.mid.to_f64(),
            max: self.max.to_f64(),
        }
    }
}

/// A dead simple fixed-length circular buffer, useful for spotting patterns in
/// lines of pixels. Write with `push`, read with `iter` or `peek_back`.
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

/// Ratios of sizes of adjacent "chunks" of a target pattern
/// (1 black, 1 white, 3 black, 1 white, 1 black)
const TARGET_RATIOS: [f32; 4] = [1.0, 1.0/3.0, 3.0, 1.0];
const TARGET_THRESH: f32 = 0.65;

/// Confirms a line of a position target (horizontal or vertical) by iterating
/// from the center outwards. If line matches the target pattern, return the
/// line's minimum and maximum coordinates.
#[inline]
fn confirm_line<'a, B, F>(back: B, fwd: F, mid: u32) -> Option<(u32, u32)>
where
    B: Iterator<Item = &'a bool>,
    F: Iterator<Item = &'a bool>,
{
    let mut size_buf = [1; 5]; // Default of 1 avoids (unlikely) divide by 0
    let mut size_idx = 2;
    let mut color = false;
    let mut min = mid;

    // Iterate backwards from the middle
    for &px in back {
        if px != color {
            color = px;
            if size_idx == 0 { break; }
            size_idx -= 1;
        }
        size_buf[size_idx] += 1;
        min -= 1;
    }

    size_idx = 2;
    color = false;
    let mut max = mid;

    // Iterate forwards from the middle
    for &px in fwd {
        if px != color {
            color = px;
            if size_idx == 4 { break; }
            size_idx += 1;
        }
        size_buf[size_idx] += 1;
        max += 1;
    }

    // Fun with iterators - calculate chunk size ratios & confirm they match
    let is_pattern = size_buf
        .windows(2)
        .map(|win| win[0] as f32 / win[1] as f32)
        .zip(TARGET_RATIOS.iter())
        .all(|(ratio, target)| {
            let off_by = ratio - target;
            -TARGET_THRESH < off_by && off_by < TARGET_THRESH
        });
    if is_pattern {
        Some((min, max))
    } else { None }
}

/// Given a row of pixels that matches the target pattern *horizontally*,
/// confirm that it also matches *vertically*.
#[inline]
fn confirm_col(img: &Bitmap, x: u32, y: u32, width: u32) -> Option<(u32, u32)> {
    let img_width = img.width() as usize;
    let point_idx = (y * img.width() + x) as usize;
    let max = (width * img.width()) as usize;

    let max_up = point_idx.checked_sub(max).unwrap_or(x as usize);
    let back = img[max_up..point_idx].iter().rev().step_by(img_width);

    let max_down = if max > img.len() - point_idx {
        img.len() - (img_width - x as usize)
    } else {
        point_idx + max
    };
    let fwd = img[point_idx..max_down].iter().step_by(img_width);

    confirm_line(back, fwd, y)
}

#[inline]
fn confirm_row(img: &Bitmap, x: u32, y: u32, width: u32) -> Option<(u32, u32)> {
    let img_width = img.width() as usize;
    let row_idx = (y * img.width()) as usize;
    let point_idx = row_idx + x as usize;
    let width_max = width * 5 / 8; // add 25% extra margin

    let max_left = row_idx + x.saturating_sub(width_max) as usize;
    let back = img[max_left..point_idx].iter().rev();

    let max_right = if x + width_max > img.width() {
        row_idx + img_width
    } else {
        point_idx + width_max as usize
    };
    let fwd = img[point_idx..max_right].iter();

    confirm_line(back, fwd, x)
}

/// Locates position targets (the 3 big squares in the corners of a QR code) in
/// an image.
pub fn find_pos_targets(img: &Bitmap) -> Vec<Target<u32>> {
    // Stores the ratios of sizes of successive chunks of pixels
    let mut ratio_buf = FixedBuffer::<f32, 4>::new();
    // Stores the x-coords of the last few chunk edges
    let mut x_buf = FixedBuffer::<u32, 6>::new();
    // Holds any targets we find
    let mut targets = Vec::new();
    // Tracks targets that are in danger of being re-scanned
    let mut active_targets = Vec::new();

    for (y, row) in img.rows().enumerate().step_by(4) {
        let y = y as u32;
        let mut enum_row = row.enumerate();
        let mut chunk_color = !*enum_row.next().unwrap().1;
        let mut last_count = 1;
        // advance through the first chunk and save its size in last_count
        while let Some((_, &px)) = enum_row.next() {
            if px == chunk_color { break; }
            last_count += 1;
        }

        x_buf.push(last_count);
        // counts size of current chunk of black/white
        let mut count = 1;

        for (x, &px) in enum_row {
            let x = x as u32;
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
                    let t: Target<u32> = targets[i];
                    (x) < t.min.x || start_x > t.max.x
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

                // We have a row that matches - now check if the middle column matches too
                let width = x - start_x;
                let x_mid = start_x + width / 2;
                if let Some((y_min, y_max)) = confirm_col(img, x_mid, y, width) {
                    // Final check - does the middle row match as well?
                    // This also helps fine-tune the edges of the target
                    let y_mid = y_min + (y_max - y_min) / 2;
                    if let Some((x_min, x_max)) = confirm_row(img, x_mid, y_mid, width) {
                        active_targets.push(targets.len());
                        let new_target = Target::new(x_min, y_min, x_mid, y_mid, x_max, y_max);
                        targets.push(new_target);
                    }
                }
            } else {
                count += 1;
            }
        }

        // clear out any active targets that we're now entirely below
        let mut ati = 0;
        while ati < active_targets.len() {
            if y > targets[active_targets[ati]].max.y {
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

/// Helper function which turns a closure into a collection of 3 elements
#[inline]
fn collect3<T, F: Fn(usize) -> T>(f: F) -> [T; 3] {
    [f(0), f(1), f(2)]
}

/// Helper function which selects the value in the range [0, 2] which yields the
/// minimum result from the provided closure
#[inline]
fn max3<T: PartialOrd, F: Fn(usize) -> T>(f: F) -> usize {
    let list = collect3(f);
    if list[0] < list[1] {
        if list[1] < list[2] { 2 } else { 1 }
    } else {
        if list[0] < list[2] { 2 } else { 0 }
    }
}

/// Given a vector of 3 `Target`s, find the outer corners of the targets, and
/// thereby 3 out of 4 outer corners of the code. Returns `None` if `targets`
/// does not contain exactly 3 elements.
/// 
/// Corner points are guaranteed to be returned in this order: top-left,
/// top-right, bottom-left
pub fn pick_corners<T>(targets: &Vec<Target<T>>) -> Option<[Point<f64>; 3]>
where
    T: Copy + Into<f64>
{
    if targets.len() != 3 {
        return None;
    }

    // Convert target coordinates to floats
    let t = collect3(|i| targets[i].to_f64());

    // Slopes from one target to the next
    let slopes = collect3(|i| {
        let Target {mid: m1, ..} = t[i];
        let Target {mid: m2, ..} = t[(i + 1) % 3];
        (m2.y - m1.y) / (m2.x - m1.x)
    });

    // Convert slopes to angles
    // (we didn't do this conversion in the first place b.c. slopes are used later)
    let angles = collect3(|i| {
        let t1 = t[i];
        let t2 = t[(i + 1) % 3];
        slopes[i].atan() + if t1.mid.x > t2.mid.x { PI*1.5 } else { PI*0.5 }
    });
    
    // Compute arcs of angles from each target to the other two
    let arcs = collect3(|i| {
        let a1 = angles[i];
        let a2 = (angles[(i + 2) % 3] + PI) % TAU;
        a2 - a1
    });

    // Index of the target whose inner arc is largest (assume this is the
    // top-left corner)
    // TODO: implement some way to confirm or refute this by checking the image.
    // (this assumption yields nonsense with images from very acute angles)
    let tl_index = max3(|i| {
        let arc = arcs[i].abs();
        if arc > PI { TAU - arc } else { arc }
    });
    let top_left = t[tl_index];
    
    // Now that we have the top-left, pick top-right and bottom-left, and which
    // slopes correspond to which edges.
    let idx = |i| (tl_index + i) % 3;
    let (top_right, bot_left, h_slope, v_slope) = if (arcs[tl_index] + TAU) % TAU > PI {
        (t[idx(2)], t[idx(1)], slopes[idx(2)], slopes[tl_index])
    } else {
        (t[idx(1)], t[idx(2)], slopes[tl_index], slopes[idx(2)])
    };

    // Choose points on the edges of the code.
    let pick_points = |targ: Target<_>, other: Target<_>, slope: f64| {
        let pick_lr = |t: Target<_>, l| if l { t.left() } else { t.right() };
        let pick_ud = |t: Target<_>, u| if u { t.up() } else { t.down() };
        if slope.abs() > 1.0 {
            let other_is_to_right = other.mid.x > (other.mid.y - targ.mid.y) / slope + targ.mid.x;
            let corner = pick_lr(top_left, other_is_to_right);
            let near = pick_lr(targ, other_is_to_right);
            let far = pick_ud(targ, other.mid.y > targ.mid.y);
            (corner, near, far)
        } else {
            let other_is_below = other.mid.y > (other.mid.x - targ.mid.x) * slope + targ.mid.y;
            let corner = pick_ud(top_left, other_is_below);
            let near = pick_ud(targ, other_is_below);
            let far = pick_lr(targ, other.mid.x > targ.mid.x);
            (corner, near, far)
        }
    };
    let (in_top, out_top, right) = pick_points(top_right, bot_left, h_slope);
    let (in_left, out_left, bottom) = pick_points(bot_left, top_right, v_slope);

    // Compute intersections of lines on the border of the code to find the code's corners
    let intersect = |p1: Point<_>, p2: Point<_>| {
        let x = (h_slope * p1.x - v_slope * p2.x + p2.y - p1.y) / (h_slope - v_slope);
        let y = h_slope * (x - p1.x) + p1.y;
        Point::new(x, y)
    };
    Some([intersect(in_top, in_left), intersect(out_top, right), intersect(bottom, out_left)])
}

pub fn to_side_len(corners: [Point<f64>; 3]) -> f64 {
    let top_len = corners[0].dist_to(corners[1]);
    let left_len = corners[0].dist_to(corners[2]);
    if top_len > left_len { top_len } else { left_len }
}

/// Given 3 corner points of the code (from `pick_corners`), compute an affine
/// transformation matrix
// This transform is immediately inverted by `bitmap::affine_transform_chunk`,
// so we sacrifice some miniscule constant performance factor to that.
pub fn to_affine_transform(corners: [Point<f64>; 3], side_len: f64) -> [[f64; 3]; 2] {    
    let angle_h = corners[0].angle_to(corners[1]);
    let angle_v = corners[0].angle_to(corners[2]);
    let h_len = side_len / corners[0].dist_to(corners[1]);
    let v_len = side_len / corners[0].dist_to(corners[2]);
    // println!("{} {}", h_len, v_len);
    println!("{}", angle_v);

    [[angle_h.cos() * h_len, -angle_v.cos() * v_len, corners[0].x],
     [-angle_h.sin() * h_len, angle_v.sin() * v_len, corners[0].y]]
}
