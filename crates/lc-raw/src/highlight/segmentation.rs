//! darktable highlight segmentation: complete scanline flood fill and morphology.
//! `src/iop/hlreconstruct/segmentation.c`, 733bd69f32cac7ff5e41025115942772add1f088.
//! Copyright (C) 2022-2026 darktable developers (Hanno Schwalm). GPL-3.0-or-later.
#![allow(unused_parens, unused_mut, non_snake_case)]
pub(super) const ID_MASK: i32 = 0x40000;
pub(super) struct Seg {
    pub data: Vec<i32>,
    pub size: Vec<i32>,
    pub xmin: Vec<i32>,
    pub xmax: Vec<i32>,
    pub ymin: Vec<i32>,
    pub ymax: Vec<i32>,
    pub val1: Vec<f32>,
    pub val2: Vec<f32>,
    pub nr: i32,
    pub border: i32,
    pub slots: i32,
    pub width: i32,
    pub height: i32,
}
impl Seg {
    pub fn new(width: usize, height: usize, border: usize, slots: usize) -> Self {
        let slots = slots.clamp(256, (ID_MASK - 2) as usize);
        Self {
            data: vec![0; width * height],
            size: vec![0; slots],
            xmin: vec![0; slots],
            xmax: vec![0; slots],
            ymin: vec![0; slots],
            ymax: vec![0; slots],
            val1: vec![0.0; slots],
            val2: vec![0.0; slots],
            nr: 2,
            border: border as i32,
            slots: slots as i32,
            width: width as i32,
            height: height as i32,
        }
    }
    pub fn id(&self, loc: usize) -> usize {
        if loc >= (self.width * (self.height - self.border)) as usize {
            return 0;
        }
        let id = self.data[loc] & (ID_MASK - 1);
        if id > 1 && id < self.nr { id as usize } else { 0 }
    }
    pub fn segmentize(&mut self) {
        let mut stack = Stack { el: Vec::new(), limit: (self.width * self.height / 32 - 1).max(1) as usize };
        let mut id = 2;
        for row in self.border..self.height - self.border {
            for col in self.border..self.width - self.border {
                if id >= self.slots - 2 {
                    return;
                }
                if self.data[(row * self.width + col) as usize] == 1 && floodfill(row, col, self, self.width, self.height, id, &mut stack) != 0 {
                    id += 1;
                }
            }
        }
    }
    pub fn combine(&mut self, radius: usize) {
        let (w, h, b) = (self.width as usize, self.height as usize, self.border as usize);
        borderfill(&mut self.data, w, h, b, 0);
        let mut tmp = vec![0; w * h];
        for y in b..h - b {
            for x in b..w - b {
                tmp[y * w + x] = i32::from(test(&self.data, w, x, y, radius, false));
            }
        }
        if radius > 3 {
            borderfill(&mut tmp, w, h, b, 1);
            for y in b..h - b {
                for x in b..w - b {
                    self.data[y * w + x] = i32::from(test(&tmp, w, x, y, radius - 3, true));
                }
            }
        } else {
            self.data = tmp;
        }
        borderfill(&mut self.data, w, h, b, 0);
    }
}
fn borderfill(d: &mut [i32], w: usize, h: usize, b: usize, v: i32) {
    // Retain upstream's bottom border offset (h-b-1), rather than silently moving it.
    let di = (h - b - 1) * w;
    for i in 0..b * w {
        d[i] = v;
        d[i + di] = v;
    }
    for row in b..h - b {
        for i in 0..b {
            d[row * w + i] = v;
            d[row * w + i + w - b] = v;
        }
    }
}
#[derive(Clone, Copy)]
struct Pos {
    xpos: i32,
    ypos: i32,
}
struct Stack {
    el: Vec<Pos>,
    limit: usize,
}
fn push_stack(x: i32, y: i32, stack: &mut Stack) {
    if stack.el.len() < stack.limit {
        stack.el.push(Pos { xpos: x, ypos: y });
    }
}
fn clear_segment_slot(seg: &mut Seg, id: i32) {
    if id < seg.slots {
        let i = id as usize;
        seg.size[i] = 0;
        seg.xmin[i] = 0;
        seg.xmax[i] = 0;
        seg.ymin[i] = 0;
        seg.ymax[i] = 0;
        seg.val1[i] = 0.0;
        seg.val2[i] = 0.0;
    }
}
fn floodfill(yin: i32, xin: i32, seg: &mut Seg, w: i32, h: i32, id: i32, stack: &mut Stack) -> i32 {
    if (id >= (seg.slots - 2)) {
        return 0;
    }
    let border: i32 = seg.border;
    let mut xp: i32;
    let mut yp: i32;
    let mut rp: i32;
    let mut min_x: i32 = xin;
    let mut max_x: i32 = xin;
    let mut min_y: i32 = yin;
    let mut max_y: i32 = yin;
    let mut cnt: i32 = 0;
    stack.el.clear();
    clear_segment_slot(seg, id);
    push_stack(xin, yin, stack);
    while let Some(coord) = stack.el.pop() {
        let x: i32 = coord.xpos;
        let y: i32 = coord.ypos;
        if (seg.data[((y * w) + x) as usize] == 1) {
            let mut yUp: i32 = (y - 1);
            let mut yDown: i32 = (y + 1);
            let mut lastXUp: i32 = 0;
            let mut lastXDown: i32 = 0;
            let mut firstXUp: i32 = 0;
            let mut firstXDown: i32 = 0;
            seg.data[((y * w) + x) as usize] = id;
            cnt += 1;
            if ((yUp >= border) && (seg.data[((yUp * w) + x) as usize] == 1)) {
                push_stack(x, yUp, stack);
                lastXUp = 1;
                firstXUp = lastXUp;
            } else {
                xp = x;
                yp = yUp;
                rp = ((yp * w) + xp);
                if ((xp > (border + 1)) && (seg.data[(rp) as usize] == 0)) {
                    min_x = (min_x).min(xp);
                    max_x = (max_x).max(xp);
                    min_y = (min_y).min(yp);
                    max_y = (max_y).max(yp);
                    seg.data[(rp) as usize] = (ID_MASK | id);
                }
            }
            if ((yDown < (h - border)) && (seg.data[((yDown * w) + x) as usize] == 1)) {
                push_stack(x, yDown, stack);
                lastXDown = 1;
                firstXDown = lastXDown;
            } else {
                xp = x;
                yp = yDown;
                rp = ((yp * w) + xp);
                if ((yp < ((h - border) - 2)) && (seg.data[(rp) as usize] == 0)) {
                    min_x = (min_x).min(xp);
                    max_x = (max_x).max(xp);
                    min_y = (min_y).min(yp);
                    max_y = (max_y).max(yp);
                    seg.data[(rp) as usize] = (ID_MASK | id);
                }
            }
            let mut xr: i32 = (x + 1);
            while ((xr < (w - border)) && (seg.data[((y * w) + xr) as usize] == 1)) {
                seg.data[((y * w) + xr) as usize] = id;
                cnt += 1;
                if ((yUp >= border) && (seg.data[((yUp * w) + xr) as usize] == 1)) {
                    if !(lastXUp != 0) {
                        push_stack(xr, yUp, stack);
                        lastXUp = 1;
                    }
                } else {
                    xp = xr;
                    yp = yUp;
                    rp = ((yp * w) + xp);
                    if ((yp > (border + 1)) && (seg.data[(rp) as usize] == 0)) {
                        min_x = (min_x).min(xp);
                        max_x = (max_x).max(xp);
                        min_y = (min_y).min(yp);
                        max_y = (max_y).max(yp);
                        seg.data[(rp) as usize] = (ID_MASK | id);
                    }
                    lastXUp = 0;
                }
                if ((yDown < (h - border)) && (seg.data[((yDown * w) + xr) as usize] == 1)) {
                    if !(lastXDown != 0) {
                        push_stack(xr, yDown, stack);
                        lastXDown = 1;
                    }
                } else {
                    xp = xr;
                    yp = yDown;
                    rp = ((yp * w) + xp);
                    if ((yp < ((h - border) - 2)) && (seg.data[(rp) as usize] == 0)) {
                        min_x = (min_x).min(xp);
                        max_x = (max_x).max(xp);
                        min_y = (min_y).min(yp);
                        max_y = (max_y).max(yp);
                        seg.data[(rp) as usize] = (ID_MASK | id);
                    }
                    lastXDown = 0;
                }
                xr += 1;
            }
            xp = xr;
            yp = y;
            rp = ((yp * w) + xp);
            if ((xp < ((w - border) - 2)) && (seg.data[(rp) as usize] == 0)) {
                min_x = (min_x).min(xp);
                max_x = (max_x).max(xp);
                min_y = (min_y).min(yp);
                max_y = (max_y).max(yp);
                seg.data[(rp) as usize] = (ID_MASK | id);
            }
            let mut xl: i32 = (x - 1);
            lastXUp = firstXUp;
            lastXDown = firstXDown;
            while ((xl >= border) && (seg.data[((y * w) + xl) as usize] == 1)) {
                seg.data[((y * w) + xl) as usize] = id;
                cnt += 1;
                if ((yUp >= border) && (seg.data[((yUp * w) + xl) as usize] == 1)) {
                    if !(lastXUp != 0) {
                        push_stack(xl, yUp, stack);
                        lastXUp = 1;
                    }
                } else {
                    xp = xl;
                    yp = yUp;
                    rp = ((yp * w) + xp);
                    if ((yp > (border + 1)) && (seg.data[(rp) as usize] == 0)) {
                        min_x = (min_x).min(xp);
                        max_x = (max_x).max(xp);
                        min_y = (min_y).min(yp);
                        max_y = (max_y).max(yp);
                        seg.data[(rp) as usize] = (ID_MASK | id);
                    }
                    lastXUp = 0;
                }
                if ((yDown < (h - border)) && (seg.data[((yDown * w) + xl) as usize] == 1)) {
                    if !(lastXDown != 0) {
                        push_stack(xl, yDown, stack);
                        lastXDown = 1;
                    }
                } else {
                    xp = xl;
                    yp = yDown;
                    rp = ((yp * w) + xp);
                    if ((yp < ((h - border) - 2)) && (seg.data[(rp) as usize] == 0)) {
                        min_x = (min_x).min(xp);
                        max_x = (max_x).max(xp);
                        min_y = (min_y).min(yp);
                        max_y = (max_y).max(yp);
                        seg.data[(rp) as usize] = (ID_MASK | id);
                    }
                    lastXDown = 0;
                }
                xl -= 1;
            }
            seg.data[((y * w) + x) as usize] = id;
            xp = xl;
            yp = y;
            rp = ((yp * w) + xp);
            if ((xp > (border + 1)) && (seg.data[(rp) as usize] == 0)) {
                min_x = (min_x).min(xp);
                max_x = (max_x).max(xp);
                min_y = (min_y).min(yp);
                max_y = (max_y).max(yp);
                seg.data[(rp) as usize] = (ID_MASK | id);
            }
        }
    }
    let success: i32 = ((cnt > 3) as i32);
    if !(success != 0) {
        {
            let mut row: i32 = min_y;
            while (row <= max_y) {
                {
                    {
                        let mut col: i32 = min_x;
                        while (col <= max_x) {
                            {
                                let mut loc: i32 = ((w * row) + col);
                                if (seg.data[(loc) as usize] == id) {
                                    seg.data[(loc) as usize] = 1;
                                } else {
                                    if (seg.data[(loc) as usize] == (id | ID_MASK)) {
                                        seg.data[(loc) as usize] = 0;
                                    }
                                }
                            }
                            col += 1;
                        }
                    }
                }
                row += 1;
            }
        }
    } else {
        seg.size[(id) as usize] = cnt;
        seg.xmin[(id) as usize] = min_x;
        seg.xmax[(id) as usize] = max_x;
        seg.ymin[(id) as usize] = min_y;
        seg.ymax[(id) as usize] = max_y;
        seg.nr += 1;
        clear_segment_slot(seg, (id + 1));
    }
    success
}
const DILATE: &[&[(isize, isize)]] = &[
    &[(-1, -1), (0, -1), (1, -1), (-1, 0), (0, 0), (1, 0), (-1, 1), (0, 1), (1, 1)],
    &[(-1, -2), (0, -2), (1, -2), (-2, -1), (2, -1), (-2, 0), (2, 0), (-2, 1), (2, 1), (-1, 2), (0, 2), (1, 2)],
    &[
        (-2, -3),
        (-1, -3),
        (0, -3),
        (1, -3),
        (2, -3),
        (-3, -2),
        (-2, -2),
        (2, -2),
        (3, -2),
        (-3, -1),
        (3, -1),
        (-3, 0),
        (3, 0),
        (-3, 1),
        (3, 1),
        (-3, 2),
        (-2, 2),
        (2, 2),
        (3, 2),
        (-2, 3),
        (-1, 3),
        (0, 3),
        (1, 3),
        (2, 3),
    ],
    &[
        (-2, -4),
        (-1, -4),
        (0, -4),
        (1, -4),
        (2, -4),
        (-3, -3),
        (3, -3),
        (-4, -2),
        (4, -2),
        (-4, -1),
        (4, -1),
        (-4, 0),
        (4, 0),
        (-4, 1),
        (4, 1),
        (-4, 2),
        (4, 2),
        (-3, 3),
        (3, 3),
        (-2, 4),
        (-1, 4),
        (0, 4),
        (1, 4),
        (2, 4),
    ],
    &[
        (-2, -5),
        (-1, -5),
        (0, -5),
        (1, -5),
        (2, -5),
        (-4, -4),
        (-3, -4),
        (3, -4),
        (4, -4),
        (-4, -3),
        (4, -3),
        (-5, -2),
        (5, -2),
        (-5, -1),
        (5, -1),
        (-5, 0),
        (5, 0),
        (-5, 1),
        (5, 1),
        (-5, 2),
        (5, 2),
        (-4, 3),
        (4, 3),
        (-4, 4),
        (-3, 4),
        (3, 4),
        (4, 4),
        (-2, 5),
        (-1, 5),
        (0, 5),
        (1, 5),
        (2, 5),
    ],
    &[
        (-2, -6),
        (-1, -6),
        (0, -6),
        (1, -6),
        (2, -6),
        (-4, -5),
        (-3, -5),
        (3, -5),
        (4, -5),
        (-5, -4),
        (5, -4),
        (-5, -3),
        (5, -3),
        (-6, -2),
        (6, -2),
        (-6, -1),
        (6, -1),
        (-6, 0),
        (6, 0),
        (-6, 1),
        (6, 1),
        (-6, 2),
        (6, 2),
        (-5, 3),
        (5, 3),
        (-5, 4),
        (5, 4),
        (-4, 5),
        (-3, 5),
        (3, 5),
        (4, 5),
        (-2, 6),
        (-1, 6),
        (0, 6),
        (1, 6),
        (2, 6),
    ],
    &[
        (-3, -7),
        (-2, -7),
        (-1, -7),
        (0, -7),
        (1, -7),
        (2, -7),
        (3, -7),
        (-4, -6),
        (-3, -6),
        (3, -6),
        (4, -6),
        (-6, -5),
        (-5, -5),
        (5, -5),
        (6, -5),
        (-6, -4),
        (6, -4),
        (-7, -3),
        (-6, -3),
        (6, -3),
        (7, -3),
        (-7, -2),
        (7, -2),
        (-7, -1),
        (7, -1),
        (-7, 0),
        (7, 0),
        (-7, 1),
        (7, 1),
        (-7, 2),
        (7, 2),
        (-7, 3),
        (-6, 3),
        (6, 3),
        (7, 3),
        (-6, 4),
        (6, 4),
        (-6, 5),
        (-5, 5),
        (5, 5),
        (6, 5),
        (-4, 6),
        (-3, 6),
        (3, 6),
        (4, 6),
        (-3, 7),
        (-2, 7),
        (-1, 7),
        (0, 7),
        (1, 7),
        (2, 7),
        (3, 7),
    ],
    &[
        (-4, -8),
        (-3, -8),
        (-2, -8),
        (-1, -8),
        (0, -8),
        (1, -8),
        (2, -8),
        (3, -8),
        (4, -8),
        (-6, -7),
        (-5, -7),
        (-4, -7),
        (4, -7),
        (5, -7),
        (6, -7),
        (-6, -6),
        (-5, -6),
        (5, -6),
        (6, -6),
        (-7, -5),
        (6, -5),
        (-8, -4),
        (-7, -4),
        (7, -4),
        (8, -4),
        (-8, -3),
        (-7, -3),
        (7, -3),
        (8, -3),
        (-8, -2),
        (8, -2),
        (-8, -1),
        (8, -1),
        (-8, 0),
        (8, 0),
        (-8, 1),
        (8, 1),
        (-8, 2),
        (8, 2),
        (-8, 3),
        (-7, 3),
        (7, 3),
        (8, 3),
        (-8, 4),
        (-7, 4),
        (7, 4),
        (8, 4),
        (-7, 5),
        (7, 5),
        (-6, 6),
        (-5, 6),
        (5, 6),
        (6, 6),
        (-6, 7),
        (-5, 7),
        (-4, 7),
        (4, 7),
        (5, 7),
        (6, 7),
        (-4, 8),
        (-3, 8),
        (-2, 8),
        (-1, 8),
        (0, 8),
        (1, 8),
        (2, 8),
        (3, 8),
        (4, 8),
    ],
];
const ERODE: &[&[(isize, isize)]] = &[
    &[(-1, -1), (0, -1), (1, -1), (-1, 0), (0, 0), (1, 0), (-1, 1), (0, 1), (1, 1)],
    &[(-1, -2), (0, -2), (1, -2), (-2, -1), (2, -1), (-2, 0), (2, 0), (-2, 1), (2, 1), (-1, 2), (0, 2), (1, 2)],
    &[
        (-2, -3),
        (-1, -3),
        (0, -3),
        (1, -3),
        (2, -3),
        (-3, -2),
        (-2, -2),
        (2, -2),
        (3, -2),
        (-3, -1),
        (3, -1),
        (-3, 0),
        (3, 0),
        (-3, 1),
        (3, 1),
        (-3, 2),
        (-2, 2),
        (2, 2),
        (3, 2),
        (-2, 3),
        (-1, 3),
        (0, 3),
        (1, 3),
        (2, 3),
    ],
    &[
        (-2, -4),
        (-1, -4),
        (0, -4),
        (1, -4),
        (2, -4),
        (-3, -3),
        (3, -3),
        (-4, -2),
        (4, -2),
        (-4, -1),
        (4, -1),
        (-4, 0),
        (4, 0),
        (-4, 1),
        (4, 1),
        (-4, 2),
        (4, 2),
        (-3, 3),
        (3, 3),
        (-2, 4),
        (-1, 4),
        (0, 4),
        (1, 4),
        (2, 4),
    ],
    &[
        (-2, -5),
        (-1, -5),
        (0, -5),
        (1, -5),
        (2, -5),
        (-4, -4),
        (-3, -4),
        (3, -4),
        (4, -4),
        (-4, -3),
        (4, -3),
        (-5, -2),
        (5, -2),
        (-5, -1),
        (5, -1),
        (-5, 0),
        (5, 0),
        (-5, 1),
        (5, 1),
        (-5, 2),
        (5, 2),
        (-4, 3),
        (4, 3),
        (-4, 4),
        (-3, 4),
        (3, 4),
        (4, 4),
        (-2, 5),
        (-1, 5),
        (0, 5),
        (1, 5),
        (2, 5),
    ],
];
fn test(img: &[i32], w: usize, x: usize, y: usize, radius: usize, erode: bool) -> bool {
    let rings = if erode { ERODE } else { DILATE };
    for ring in rings.iter().take(radius.max(1)) {
        let hit = if erode {
            ring.iter().all(|&(dx, dy)| img[((y as isize + dy) * w as isize + x as isize + dx) as usize] != 0)
        } else {
            ring.iter().any(|&(dx, dy)| img[((y as isize + dy) * w as isize + x as isize + dx) as usize] != 0)
        };
        if hit != erode {
            return hit;
        }
    }
    erode
}
