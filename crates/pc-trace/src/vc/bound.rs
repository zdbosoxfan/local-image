//! Region bounds for hierarchical image tracing.
//!
//! Ported from visioncortex @ 0062088c89645aac76c00e066deb7e8f53980dd7.
// Copyright (c) 2026 TSANG, Hao Fung, visioncortex contributors.
// SPDX-License-Identifier: MIT OR Apache-2.0
// See licenses/visioncortex-LICENSE-MIT and -APACHE.
#[derive(Copy, Clone, Default, Debug)]
pub struct BoundingRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}
impl BoundingRect {
    pub fn width(self) -> i32 {
        self.right - self.left
    }
    pub fn height(self) -> i32 {
        self.bottom - self.top
    }
    pub fn is_empty(self) -> bool {
        self.width() == 0 && self.height() == 0
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub fn add_x_y(&mut self, x: i32, y: i32) {
        if self.is_empty() {
            self.left = x;
            self.right = x + 1;
            self.top = y;
            self.bottom = y + 1;
            return;
        }
        if x < self.left {
            self.left = x;
        } else if x + 1 > self.right {
            self.right = x + 1;
        }
        if y < self.top {
            self.top = y;
        } else if y + 1 > self.bottom {
            self.bottom = y + 1;
        }
    }

    /// Expands `self` to be the smallest rect that encloses both `self` and `other`.
    ///
    /// If `other` is empty it is ignored. If `self` is empty it is replaced by `other`.
    ///
    /// ```text
    /// ┌────┐            ┌──────────┐
    /// │self│   merge    │          │
    /// └────┘   ──────►  │          │
    ///      ┌─────┐      │          │
    ///      │other│      │          │
    ///      └─────┘      └──────────┘
    /// ```
    pub fn merge(&mut self, other: Self) {
        if other.is_empty() {
            return;
        }
        if self.is_empty() {
            self.left = other.left;
            self.right = other.right;
            self.top = other.top;
            self.bottom = other.bottom;
            return;
        }
        self.left = std::cmp::min(self.left, other.left);
        self.right = std::cmp::max(self.right, other.right);
        self.top = std::cmp::min(self.top, other.top);
        self.bottom = std::cmp::max(self.bottom, other.bottom);
    }
}
