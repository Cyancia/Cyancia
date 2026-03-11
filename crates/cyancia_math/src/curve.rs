use glam::Vec2;

pub struct CubicCurve {
    control_points: Vec<Vec2>,
    derivatives: Vec<f32>,
}

impl CubicCurve {
    pub fn new(control_points: Vec<Vec2>) -> Self {
        let derivatives = Self::calculate_derivatives(&control_points);
        Self {
            control_points,
            derivatives,
        }
    }

    pub fn calculate_derivatives(points: &[Vec2]) -> Vec<f32> {
        let n = points.len() - 1;

        let mut a = vec![0.0_f32; n + 1];
        let mut b = vec![0.0_f32; n + 1];
        let mut c = vec![0.0_f32; n + 1];
        let mut d = vec![0.0_f32; n + 1];

        {
            let dx = points[1].x - points[0].x;
            b[0] = 2.0 / dx;
            c[0] = 1.0 / dx;
            d[0] = 3.0 * (points[1].y - points[0].y) / (dx * dx);
        }

        for i in 1..n {
            let dx_prev = points[i].x - points[i - 1].x;
            let dx_next = points[i + 1].x - points[i].x;

            a[i] = 1.0 / dx_prev;
            b[i] = 2.0 * (1.0 / dx_prev + 1.0 / dx_next);
            c[i] = 1.0 / dx_next;
            d[i] = 3.0
                * ((points[i].y - points[i - 1].y) / (dx_prev * dx_prev)
                    + (points[i + 1].y - points[i].y) / (dx_next * dx_next));
        }

        {
            let dx = points[n].x - points[n - 1].x;
            a[n] = 1.0 / dx;
            b[n] = 2.0 / dx;
            d[n] = 3.0 * (points[n].y - points[n - 1].y) / (dx * dx);
        }

        for i in 1..=n {
            let w = a[i] / b[i - 1];
            b[i] -= w * c[i - 1];
            d[i] -= w * d[i - 1];
        }

        d[n] /= b[n];
        for i in (0..n).rev() {
            d[i] = (d[i] - c[i] * d[i + 1]) / b[i];
        }

        d
    }

    pub fn subdivide(&self, n: usize) -> Vec<Vec2> {
        let pts = &self.control_points;
        let ks = &self.derivatives;
        let x_start = pts[0].x;
        let x_end = pts[pts.len() - 1].x;
        let inv_n = 1.0 / n as f32;
        let x_span = x_end - x_start;

        let mut seg = 1;
        let mut result = Vec::with_capacity(n + 1);

        for i in 0..=n {
            let x = x_start + x_span * i as f32 * inv_n;

            while seg < pts.len() - 1 && pts[seg].x < x {
                seg += 1;
            }

            let dx = pts[seg].x - pts[seg - 1].x;
            let t = (x - pts[seg - 1].x) / dx;
            let dy = pts[seg].y - pts[seg - 1].y;

            let a = ks[seg - 1] * dx - dy;
            let b = -ks[seg] * dx + dy;

            let y = (1.0 - t) * pts[seg - 1].y
                + t * pts[seg].y
                + t * (1.0 - t) * (a * (1.0 - t) + b * t);
            result.push(Vec2::new(x, y).clamp(Vec2::ZERO, Vec2::ONE));
        }

        result
    }

    pub fn sample(&self, x: f32) -> f32 {
        let pts = &self.control_points;
        let ks = &self.derivatives;

        let x = x.clamp(pts[0].x, pts[pts.len() - 1].x);

        let i = match pts.binary_search_by(|p| p.x.partial_cmp(&x).unwrap()) {
            Ok(0) => 1,
            Ok(i) => i,
            Err(i) => i.max(1),
        };

        let dx = pts[i].x - pts[i - 1].x;
        let t = (x - pts[i - 1].x) / dx;
        let dy = pts[i].y - pts[i - 1].y;

        let a = ks[i - 1] * dx - dy;
        let b = -ks[i] * dx + dy;

        (1.0 - t) * pts[i - 1].y
            + t * pts[i].y
            + t * (1.0 - t) * (a * (1.0 - t) + b * t).clamp(0.0, 1.0)
    }

    pub fn control_points(&self) -> &[Vec2] {
        &self.control_points
    }

    pub fn derivatives(&self) -> &[f32] {
        &self.derivatives
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::vec2;

    #[test]
    fn test_interpolates_through_control_points() {
        let points = vec![
            vec2(0.0, 0.0),
            vec2(0.25, 0.3),
            vec2(0.5, 0.6),
            vec2(0.75, 0.8),
            vec2(1.0, 1.0),
        ];
        let curve = CubicCurve::new(points.clone());

        for p in &points {
            let y = curve.sample(p.x);
            assert!(
                (y - p.y).abs() < 1e-4,
                "At x={}, expected y={}, got y={}",
                p.x,
                p.y,
                y
            );
        }
    }

    #[test]
    fn test_clamps_to_range() {
        let points = vec![vec2(0.0, 0.0), vec2(0.5, 0.7), vec2(1.0, 1.0)];
        let curve = CubicCurve::new(points);

        let y_low = curve.sample(-0.5);
        assert!((y_low - curve.sample(0.0)).abs() < 1e-6);

        let y_high = curve.sample(1.5);
        assert!((y_high - curve.sample(1.0)).abs() < 1e-6);
    }

    #[test]
    fn test_linear_passthrough() {
        let points = vec![vec2(0.0, 0.0), vec2(0.5, 0.5), vec2(1.0, 1.0)];
        let curve = CubicCurve::new(points);

        for i in 0..=20 {
            let x = i as f32 / 20.0;
            let y = curve.sample(x);
            assert!(
                (y - x).abs() < 1e-4,
                "At x={}, expected y={}, got y={}",
                x,
                x,
                y
            );
        }
    }
}
