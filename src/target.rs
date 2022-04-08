use std::{iter, slice};
use crate::bitmap::Bitmap;

#[derive(Clone, Copy, Debug, Default)]
pub struct GridLine {
    pub x: usize,
    pub y: usize,
    pub p2: usize,
    pub vert: bool,
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
const TARGET_THRESH: f32 = 0.4;

#[inline]
fn confirm_position_target(img: &Bitmap, point: GridLine) -> Option<GridLine> {
    let line_length = point.p2 - point.x;
    let center_x = point.x + line_length / 2;
    let width = img.width() as usize;
    let point_idx = point.y * width + center_x;
    let max = line_length * width;

    let mut size_buf = [0; 5];
    let mut size_idx = 2;
    let mut color = false;
    let mut y_min = point.y;

    // Iterate up, count the size of black & white chunks
    let max_up = if max > point_idx {
        center_x
    } else {
        point_idx - max
    };
    for &px in img[max_up..point_idx].iter().rev().step_by(width) {
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
    let mut y_max = point.y;

    // Iterate down, count the size of black and white chunks
    let max_down = if max > img.len() - point_idx {
        img.len() - (width - center_x)
    } else {
        point_idx + max
    };
    for &px in img[point_idx..max_down].iter().step_by(width) {
        if px != color {
            color = px;
            if size_idx == 4 { break; }
            size_idx += 1;
        }
        size_buf[size_idx] += 1;
        y_max += 1;
    }

    // nasty iterator crimes
    let is_pattern = size_buf
        .windows(2)
        .map(|win| win[0] as f32 / win[1] as f32)
        .zip(TARGET_RATIOS.iter())
        .all(|(ratio, target)| {
            let off_by = ratio - target;
            -TARGET_THRESH < off_by && off_by < TARGET_THRESH
        });

    if is_pattern {
        Some(GridLine {
            x: center_x,
            y: y_min,
            p2: y_max,
            vert: true
        })
    } else { None }
}

pub fn find_position_targets(img: &Bitmap) -> Vec<GridLine> {
    let mut ratio_buf = FixedBuffer::<f32, 4>::new();
    let mut x_buf = FixedBuffer::<usize, 6>::new();
    let mut result = Vec::new();

    for (y, row) in img.rows().enumerate().step_by(4) {
        let mut chunk_color = false;
        let mut count = 0;
        let mut last_count = 0;

        for (x, &px) in row.enumerate() {
            if px != chunk_color {
                chunk_color = px;

                x_buf.push(x);
                if last_count > 0 {
                    ratio_buf.push(last_count as f32 / count as f32);
                }

                last_count = count;
                count = 0;

                if chunk_color && ratio_buf.is_full() {
                    let is_pattern = ratio_buf
                        .iter()
                        .zip(TARGET_RATIOS.iter())
                        .all(|(ratio, target)| {
                            let off_by = ratio - target;
                            -TARGET_THRESH < off_by && off_by < TARGET_THRESH
                        });

                    if is_pattern {
                        let start_x = x_buf.peek_back();
                        let point = GridLine { x: start_x, y, p2: x, vert: false };
                        result.push(point);

                        if let Some(v_line) = confirm_position_target(img, point) {
                            result.push(v_line);
                        }
                    }
                }
            }
            count += 1;
        }
        ratio_buf.clear();
        x_buf.clear();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::FixedBuffer;

    fn buffer_from<T, const N: usize>(contents: &Vec<T>) -> FixedBuffer<T, N>
    where
        T: Copy + Default,
    {
        let mut buf = FixedBuffer::<T, N>::new();
        contents.iter().for_each(|&n| buf.push(n));
        buf
    }

    #[test]
    fn buffer_not_full() {
        let buf = buffer_from::<u32, 4>(&vec![1, 2, 3]);
        assert!(!buf.is_full());
    }

    #[test]
    fn buffer_is_full() {
        let buf = buffer_from::<u32, 4>(&vec![1, 2, 3, 4]);
        assert!(buf.is_full());
    }

    #[test]
    fn buffer_iter_stops_at_len() {
        let buf = buffer_from::<u32, 4>(&vec![1, 2, 3]);
        assert_eq!(buf.iter().count(), 3);
    }

    #[test]
    fn buffer_values_wrap() {
        let buf = buffer_from::<u32, 4>(&vec![1, 2, 3, 4, 5, 6]);
        // println!("{:?}", buf);
        // println!("{:?}", buf.iter().collect::<Vec<_>>());
        let mut iter = buf.iter();
        assert_eq!(iter.next(), Some(&3));
        assert_eq!(iter.next(), Some(&4));
        assert_eq!(iter.next(), Some(&5));
        assert_eq!(iter.next(), Some(&6));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn buffer_can_be_reused() {
        let mut buf = buffer_from::<u32, 4>(&vec![1, 2, 3, 4, 5, 6]);
        buf.clear();
        [7, 8, 9].into_iter().for_each(|n| buf.push(n));
        // println!("{:?}", buf);
        // println!("{:?}", buf.iter().collect::<Vec<_>>());
        let mut iter = buf.iter();
        assert_eq!(iter.next(), Some(&7));
        assert_eq!(iter.next(), Some(&8));
        assert_eq!(iter.next(), Some(&9));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn buffer_peek_not_full() {
        let buf = buffer_from::<u32, 4>(&vec![1, 2, 3]);
        assert_eq!(buf.peek_back(), 1); 
    }

    #[test]
    fn buffer_peek_full() {
        let buf = buffer_from::<u32, 4>(&vec![1, 2, 3, 4, 5]);
        assert_eq!(buf.peek_back(), 2);
    }

    // use image::io::Reader as ImageReader;
    // #[test]
    // fn finds_test_target() {
    //     let img = ImageReader::open("assets/test_target.png")
    //         .unwrap().decode().unwrap();
    //     let targets = find_position_targets(img.to_luma8());
    //     println!("{:?}", targets);
    //     assert_ne!(targets.iter().count(), 0);
    // }
}
