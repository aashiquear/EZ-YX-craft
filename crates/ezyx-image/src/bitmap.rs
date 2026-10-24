//! Binary images: morphology, speck removal, thinning (Zhang–Suen), Euclidean distance transform.

#[derive(Debug, Clone, PartialEq)]
pub struct Bitmap {
    pub width: usize,
    pub height: usize,
    pub data: Vec<bool>,
}

impl Bitmap {
    pub fn new(width: usize, height: usize) -> Self {
        Self { width, height, data: vec![false; width * height] }
    }
    #[inline]
    pub fn get(&self, x: isize, y: isize) -> bool {
        x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height && self.data[y as usize * self.width + x as usize]
    }
    #[inline]
    pub fn set(&mut self, x: usize, y: usize, v: bool) {
        self.data[y * self.width + x] = v;
    }
    pub fn count(&self) -> usize {
        self.data.iter().filter(|b| **b).count()
    }
    pub fn invert(&mut self) {
        for b in &mut self.data {
            *b = !*b;
        }
    }

    fn morph(&self, r: isize, dilate: bool) -> Bitmap {
        let mut out = Bitmap::new(self.width, self.height);
        for y in 0..self.height as isize {
            for x in 0..self.width as isize {
                let mut v = !dilate;
                'k: for dy in -r..=r {
                    for dx in -r..=r {
                        if dx * dx + dy * dy > r * r {
                            continue;
                        }
                        let s = self.get(x + dx, y + dy);
                        if dilate && s {
                            v = true;
                            break 'k;
                        }
                        if !dilate && !s {
                            v = false;
                            break 'k;
                        }
                    }
                }
                out.set(x as usize, y as usize, v);
            }
        }
        out
    }
    pub fn dilate(&self, r: usize) -> Bitmap {
        self.morph(r as isize, true)
    }
    pub fn erode(&self, r: usize) -> Bitmap {
        self.morph(r as isize, false)
    }
    /// Close small gaps in pen strokes.
    pub fn close(&self, r: usize) -> Bitmap {
        self.dilate(r).erode(r)
    }
    /// Remove thin noise.
    pub fn open(&self, r: usize) -> Bitmap {
        self.erode(r).dilate(r)
    }

    /// Remove 8-connected components smaller than `min_pixels` (dust, paper texture).
    pub fn remove_specks(&mut self, min_pixels: usize) {
        let (w, h) = (self.width, self.height);
        let mut seen = vec![false; w * h];
        let mut stack = Vec::new();
        for start in 0..w * h {
            if !self.data[start] || seen[start] {
                continue;
            }
            let mut comp = Vec::new();
            stack.push(start);
            seen[start] = true;
            while let Some(i) = stack.pop() {
                comp.push(i);
                let (x, y) = ((i % w) as isize, (i / w) as isize);
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let (nx, ny) = (x + dx, y + dy);
                        if self.get(nx, ny) {
                            let j = ny as usize * w + nx as usize;
                            if !seen[j] {
                                seen[j] = true;
                                stack.push(j);
                            }
                        }
                    }
                }
            }
            if comp.len() < min_pixels {
                for i in comp {
                    self.data[i] = false;
                }
            }
        }
    }

    /// Zhang–Suen thinning to a 1-pixel skeleton.
    pub fn skeleton(&self) -> Bitmap {
        let mut img = self.clone();
        let (w, h) = (self.width as isize, self.height as isize);
        loop {
            let mut changed = false;
            for step in 0..2 {
                let mut kill = Vec::new();
                for y in 0..h {
                    for x in 0..w {
                        if !img.get(x, y) {
                            continue;
                        }
                        let p = [
                            img.get(x, y - 1),
                            img.get(x + 1, y - 1),
                            img.get(x + 1, y),
                            img.get(x + 1, y + 1),
                            img.get(x, y + 1),
                            img.get(x - 1, y + 1),
                            img.get(x - 1, y),
                            img.get(x - 1, y - 1),
                        ];
                        let b = p.iter().filter(|v| **v).count();
                        if !(2..=6).contains(&b) {
                            continue;
                        }
                        let a = (0..8).filter(|&i| !p[i] && p[(i + 1) % 8]).count();
                        if a != 1 {
                            continue;
                        }
                        let ok =
                            if step == 0 { !(p[0] && p[2] && p[4]) && !(p[2] && p[4] && p[6]) } else { !(p[0] && p[2] && p[6]) && !(p[0] && p[4] && p[6]) };
                        if ok {
                            kill.push((x as usize, y as usize));
                        }
                    }
                }
                if !kill.is_empty() {
                    changed = true;
                }
                for (x, y) in kill {
                    img.set(x, y, false);
                }
            }
            if !changed {
                break;
            }
        }
        img
    }

    /// Exact Euclidean distance (pixels) from every foreground pixel to the nearest background
    /// pixel (Felzenszwalb–Huttenlocher separable algorithm). Background pixels get 0.
    pub fn distance_transform(&self) -> Vec<f32> {
        let (w, h) = (self.width, self.height);
        const INF: f64 = 1e20;
        let mut f: Vec<f64> = self.data.iter().map(|b| if *b { INF } else { 0.0 }).collect();
        let mut tmp = vec![0f64; w.max(h)];
        // Columns.
        for x in 0..w {
            let col: Vec<f64> = (0..h).map(|y| f[y * w + x]).collect();
            edt_1d(&col, &mut tmp[..h]);
            for y in 0..h {
                f[y * w + x] = tmp[y];
            }
        }
        // Rows.
        for y in 0..h {
            let row: Vec<f64> = f[y * w..(y + 1) * w].to_vec();
            edt_1d(&row, &mut tmp[..w]);
            f[y * w..(y + 1) * w].copy_from_slice(&tmp[..w]);
        }
        f.into_iter().map(|v| v.sqrt() as f32).collect()
    }
}

fn edt_1d(f: &[f64], d: &mut [f64]) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let mut v = vec![0usize; n];
    let mut z = vec![0f64; n + 1];
    let mut k = 0usize;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    for q in 1..n {
        loop {
            let p = v[k];
            let s = ((f[q] + (q * q) as f64) - (f[p] + (p * p) as f64)) / (2.0 * q as f64 - 2.0 * p as f64);
            if s <= z[k] && k > 0 {
                k -= 1;
                continue;
            }
            if s <= z[k] {
                // k == 0: replace
                v[0] = q;
                z[0] = f64::NEG_INFINITY;
                z[1] = f64::INFINITY;
                break;
            }
            k += 1;
            v[k] = q;
            z[k] = s;
            z[k + 1] = f64::INFINITY;
            break;
        }
    }
    k = 0;
    for (q, out) in d.iter_mut().enumerate().take(n) {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let p = v[k];
        *out = (q as f64 - p as f64).powi(2) + f[p];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edt_square() {
        let mut b = Bitmap::new(11, 11);
        for y in 1..10 {
            for x in 1..10 {
                b.set(x, y, true);
            }
        }
        let d = b.distance_transform();
        assert!((d[5 * 11 + 5] - 5.0).abs() < 1e-5, "{}", d[5 * 11 + 5]);
        assert!((d[11 + 1] - 1.0).abs() < 1e-5);
        assert_eq!(d[0], 0.0);
    }
    #[test]
    fn skeleton_of_bar_is_thin() {
        let mut b = Bitmap::new(40, 11);
        for y in 3..8 {
            for x in 5..35 {
                b.set(x, y, true);
            }
        }
        let s = b.skeleton();
        assert!(s.count() > 15 && s.count() < 40, "{}", s.count());
    }
    #[test]
    fn specks() {
        let mut b = Bitmap::new(10, 10);
        b.set(1, 1, true);
        for x in 3..9 {
            b.set(x, 5, true);
        }
        b.remove_specks(3);
        assert_eq!(b.count(), 6);
    }
}
