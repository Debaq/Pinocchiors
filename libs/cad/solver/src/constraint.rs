use crate::types::Point2;

/// Restricción geométrica entre entidades de un sketch.
/// Cada variante genera una o más ecuaciones f_i(x) = 0.
#[derive(Debug, Clone)]
pub enum Constraint {
    /// Dos puntos coinciden: p1 == p2
    Coincident { p1_idx: usize, p2_idx: usize },

    /// Distancia entre dos puntos: |p1 - p2| = distance
    Distance {
        p1_idx: usize,
        p2_idx: usize,
        distance: f64,
    },

    /// Distancia horizontal: p1.x - p2.x = distance
    HorizontalDist {
        p1_idx: usize,
        p2_idx: usize,
        distance: f64,
    },

    /// Distancia vertical: p1.y - p2.y = distance
    VerticalDist {
        p1_idx: usize,
        p2_idx: usize,
        distance: f64,
    },

    /// Punto fijo en posición: p == (x, y)
    Fixed {
        p_idx: usize,
        position: Point2,
    },

    /// Línea horizontal: p1.y == p2.y
    Horizontal { p1_idx: usize, p2_idx: usize },

    /// Línea vertical: p1.x == p2.x
    Vertical { p1_idx: usize, p2_idx: usize },

    /// Perpendicular entre dos líneas (definidas por 4 puntos)
    Perpendicular {
        l1_p1: usize,
        l1_p2: usize,
        l2_p1: usize,
        l2_p2: usize,
    },

    /// Paralelo entre dos líneas
    Parallel {
        l1_p1: usize,
        l1_p2: usize,
        l2_p1: usize,
        l2_p2: usize,
    },

    /// Ángulo entre dos líneas
    Angle {
        l1_p1: usize,
        l1_p2: usize,
        l2_p1: usize,
        l2_p2: usize,
        angle_rad: f64,
    },

    /// Tangencia entre línea y círculo
    TangentLineCircle {
        line_p1: usize,
        line_p2: usize,
        center_idx: usize,
        radius: f64,
    },

    /// Longitudes iguales de dos líneas
    EqualLength {
        l1_p1: usize,
        l1_p2: usize,
        l2_p1: usize,
        l2_p2: usize,
    },

    /// Punto en el punto medio de una línea
    Midpoint {
        p_idx: usize,
        line_p1: usize,
        line_p2: usize,
    },

    /// Simetría de dos puntos respecto a una línea
    Symmetric {
        p1_idx: usize,
        p2_idx: usize,
        line_p1: usize,
        line_p2: usize,
    },

    /// Punto sobre línea
    PointOnLine {
        p_idx: usize,
        line_p1: usize,
        line_p2: usize,
    },

    /// Punto sobre círculo
    PointOnCircle {
        p_idx: usize,
        center_idx: usize,
        radius: f64,
    },

    /// Punto sobre spline (polilínea definida por puntos de control)
    /// El punto debe estar sobre el segmento más cercano de la polilínea
    PointOnSpline {
        p_idx: usize,
        spline_point_indices: Vec<usize>,
    },

    /// Radio fijo de un círculo (referenciado por distancia centro-punto en circunferencia)
    Radius {
        center_idx: usize,
        p_on_circle: usize,
        radius: f64,
    },
}

impl Constraint {
    /// Número de ecuaciones escalares que genera esta restricción
    pub fn num_equations(&self) -> usize {
        match self {
            Constraint::Coincident { .. } => 2,
            Constraint::Distance { .. } => 1,
            Constraint::HorizontalDist { .. } => 1,
            Constraint::VerticalDist { .. } => 1,
            Constraint::Fixed { .. } => 2,
            Constraint::Horizontal { .. } => 1,
            Constraint::Vertical { .. } => 1,
            Constraint::Perpendicular { .. } => 1,
            Constraint::Parallel { .. } => 1,
            Constraint::Angle { .. } => 1,
            Constraint::TangentLineCircle { .. } => 1,
            Constraint::EqualLength { .. } => 1,
            Constraint::Midpoint { .. } => 2,
            Constraint::Symmetric { .. } => 2,
            Constraint::PointOnLine { .. } => 1,
            Constraint::PointOnCircle { .. } => 1,
            Constraint::PointOnSpline { .. } => 2,
            Constraint::Radius { .. } => 1,
        }
    }

    /// Índices de los puntos que esta restricción afecta
    pub fn point_indices(&self) -> Vec<usize> {
        match self {
            Constraint::Coincident { p1_idx, p2_idx } => vec![*p1_idx, *p2_idx],
            Constraint::Distance {
                p1_idx, p2_idx, ..
            } => vec![*p1_idx, *p2_idx],
            Constraint::HorizontalDist {
                p1_idx, p2_idx, ..
            } => vec![*p1_idx, *p2_idx],
            Constraint::VerticalDist {
                p1_idx, p2_idx, ..
            } => vec![*p1_idx, *p2_idx],
            Constraint::Fixed { p_idx, .. } => vec![*p_idx],
            Constraint::Horizontal { p1_idx, p2_idx } => vec![*p1_idx, *p2_idx],
            Constraint::Vertical { p1_idx, p2_idx } => vec![*p1_idx, *p2_idx],
            Constraint::Perpendicular {
                l1_p1,
                l1_p2,
                l2_p1,
                l2_p2,
            } => vec![*l1_p1, *l1_p2, *l2_p1, *l2_p2],
            Constraint::Parallel {
                l1_p1,
                l1_p2,
                l2_p1,
                l2_p2,
            } => vec![*l1_p1, *l1_p2, *l2_p1, *l2_p2],
            Constraint::Angle {
                l1_p1,
                l1_p2,
                l2_p1,
                l2_p2,
                ..
            } => vec![*l1_p1, *l1_p2, *l2_p1, *l2_p2],
            Constraint::TangentLineCircle {
                line_p1,
                line_p2,
                center_idx,
                ..
            } => vec![*line_p1, *line_p2, *center_idx],
            Constraint::EqualLength {
                l1_p1,
                l1_p2,
                l2_p1,
                l2_p2,
            } => vec![*l1_p1, *l1_p2, *l2_p1, *l2_p2],
            Constraint::Midpoint {
                p_idx,
                line_p1,
                line_p2,
            } => vec![*p_idx, *line_p1, *line_p2],
            Constraint::Symmetric {
                p1_idx,
                p2_idx,
                line_p1,
                line_p2,
            } => vec![*p1_idx, *p2_idx, *line_p1, *line_p2],
            Constraint::PointOnLine {
                p_idx,
                line_p1,
                line_p2,
            } => vec![*p_idx, *line_p1, *line_p2],
            Constraint::PointOnCircle {
                p_idx,
                center_idx,
                ..
            } => vec![*p_idx, *center_idx],
            Constraint::PointOnSpline {
                p_idx,
                spline_point_indices,
            } => {
                let mut v = vec![*p_idx];
                v.extend(spline_point_indices.iter());
                v
            }
            Constraint::Radius {
                center_idx,
                p_on_circle,
                ..
            } => vec![*center_idx, *p_on_circle],
        }
    }

    /// Remapear índices de puntos según un mapa global→local.
    pub fn remap_indices(&self, map: &std::collections::HashMap<usize, usize>) -> Constraint {
        let r = |idx: &usize| -> usize { *map.get(idx).unwrap_or(idx) };

        match self {
            Constraint::Coincident { p1_idx, p2_idx } =>
                Constraint::Coincident { p1_idx: r(p1_idx), p2_idx: r(p2_idx) },
            Constraint::Distance { p1_idx, p2_idx, distance } =>
                Constraint::Distance { p1_idx: r(p1_idx), p2_idx: r(p2_idx), distance: *distance },
            Constraint::HorizontalDist { p1_idx, p2_idx, distance } =>
                Constraint::HorizontalDist { p1_idx: r(p1_idx), p2_idx: r(p2_idx), distance: *distance },
            Constraint::VerticalDist { p1_idx, p2_idx, distance } =>
                Constraint::VerticalDist { p1_idx: r(p1_idx), p2_idx: r(p2_idx), distance: *distance },
            Constraint::Fixed { p_idx, position } =>
                Constraint::Fixed { p_idx: r(p_idx), position: *position },
            Constraint::Horizontal { p1_idx, p2_idx } =>
                Constraint::Horizontal { p1_idx: r(p1_idx), p2_idx: r(p2_idx) },
            Constraint::Vertical { p1_idx, p2_idx } =>
                Constraint::Vertical { p1_idx: r(p1_idx), p2_idx: r(p2_idx) },
            Constraint::Perpendicular { l1_p1, l1_p2, l2_p1, l2_p2 } =>
                Constraint::Perpendicular { l1_p1: r(l1_p1), l1_p2: r(l1_p2), l2_p1: r(l2_p1), l2_p2: r(l2_p2) },
            Constraint::Parallel { l1_p1, l1_p2, l2_p1, l2_p2 } =>
                Constraint::Parallel { l1_p1: r(l1_p1), l1_p2: r(l1_p2), l2_p1: r(l2_p1), l2_p2: r(l2_p2) },
            Constraint::Angle { l1_p1, l1_p2, l2_p1, l2_p2, angle_rad } =>
                Constraint::Angle { l1_p1: r(l1_p1), l1_p2: r(l1_p2), l2_p1: r(l2_p1), l2_p2: r(l2_p2), angle_rad: *angle_rad },
            Constraint::TangentLineCircle { line_p1, line_p2, center_idx, radius } =>
                Constraint::TangentLineCircle { line_p1: r(line_p1), line_p2: r(line_p2), center_idx: r(center_idx), radius: *radius },
            Constraint::EqualLength { l1_p1, l1_p2, l2_p1, l2_p2 } =>
                Constraint::EqualLength { l1_p1: r(l1_p1), l1_p2: r(l1_p2), l2_p1: r(l2_p1), l2_p2: r(l2_p2) },
            Constraint::Midpoint { p_idx, line_p1, line_p2 } =>
                Constraint::Midpoint { p_idx: r(p_idx), line_p1: r(line_p1), line_p2: r(line_p2) },
            Constraint::Symmetric { p1_idx, p2_idx, line_p1, line_p2 } =>
                Constraint::Symmetric { p1_idx: r(p1_idx), p2_idx: r(p2_idx), line_p1: r(line_p1), line_p2: r(line_p2) },
            Constraint::PointOnLine { p_idx, line_p1, line_p2 } =>
                Constraint::PointOnLine { p_idx: r(p_idx), line_p1: r(line_p1), line_p2: r(line_p2) },
            Constraint::PointOnCircle { p_idx, center_idx, radius } =>
                Constraint::PointOnCircle { p_idx: r(p_idx), center_idx: r(center_idx), radius: *radius },
            Constraint::PointOnSpline { p_idx, spline_point_indices } =>
                Constraint::PointOnSpline { p_idx: r(p_idx), spline_point_indices: spline_point_indices.iter().map(&r).collect() },
            Constraint::Radius { center_idx, p_on_circle, radius } =>
                Constraint::Radius { center_idx: r(center_idx), p_on_circle: r(p_on_circle), radius: *radius },
        }
    }

    /// Evaluar residuales: f_i(x) que deben ser 0
    pub fn residuals(&self, points: &[Point2]) -> Vec<f64> {
        match self {
            Constraint::Coincident { p1_idx, p2_idx } => {
                let p1 = &points[*p1_idx];
                let p2 = &points[*p2_idx];
                vec![p1.x() - p2.x(), p1.y() - p2.y()]
            }

            Constraint::Distance {
                p1_idx,
                p2_idx,
                distance,
            } => {
                let p1 = &points[*p1_idx];
                let p2 = &points[*p2_idx];
                let dx = p1.x() - p2.x();
                let dy = p1.y() - p2.y();
                let len = (dx * dx + dy * dy).sqrt();
                // Reformulado: |p1-p2| - d = 0 (convergencia lineal, no cuadrática)
                vec![len - distance]
            }

            Constraint::HorizontalDist {
                p1_idx,
                p2_idx,
                distance,
            } => {
                let p1 = &points[*p1_idx];
                let p2 = &points[*p2_idx];
                vec![p1.x() - p2.x() - distance]
            }

            Constraint::VerticalDist {
                p1_idx,
                p2_idx,
                distance,
            } => {
                let p1 = &points[*p1_idx];
                let p2 = &points[*p2_idx];
                vec![p1.y() - p2.y() - distance]
            }

            Constraint::Fixed { p_idx, position } => {
                let p = &points[*p_idx];
                vec![p.x() - position.x(), p.y() - position.y()]
            }

            Constraint::Horizontal { p1_idx, p2_idx } => {
                let p1 = &points[*p1_idx];
                let p2 = &points[*p2_idx];
                vec![p1.y() - p2.y()]
            }

            Constraint::Vertical { p1_idx, p2_idx } => {
                let p1 = &points[*p1_idx];
                let p2 = &points[*p2_idx];
                vec![p1.x() - p2.x()]
            }

            Constraint::Perpendicular {
                l1_p1,
                l1_p2,
                l2_p1,
                l2_p2,
            } => {
                let d1 = points[*l1_p2].co - points[*l1_p1].co;
                let d2 = points[*l2_p2].co - points[*l2_p1].co;
                vec![d1.dot(&d2)]
            }

            Constraint::Parallel {
                l1_p1,
                l1_p2,
                l2_p1,
                l2_p2,
            } => {
                let d1 = points[*l1_p2].co - points[*l1_p1].co;
                let d2 = points[*l2_p2].co - points[*l2_p1].co;
                // Producto cruzado 2D: d1.x * d2.y - d1.y * d2.x = 0
                vec![d1.x * d2.y - d1.y * d2.x]
            }

            Constraint::Angle {
                l1_p1,
                l1_p2,
                l2_p1,
                l2_p2,
                angle_rad,
            } => {
                let d1 = points[*l1_p2].co - points[*l1_p1].co;
                let d2 = points[*l2_p2].co - points[*l2_p1].co;
                let dot = d1.dot(&d2);
                let cross = d1.x * d2.y - d1.y * d2.x;
                // atan2(cross, dot) = angle
                vec![cross.atan2(dot) - angle_rad]
            }

            Constraint::TangentLineCircle {
                line_p1,
                line_p2,
                center_idx,
                radius,
            } => {
                let lp1 = &points[*line_p1];
                let lp2 = &points[*line_p2];
                let c = &points[*center_idx];
                let d = lp2.co - lp1.co;
                let f = lp1.co - c.co;
                let len = d.dot(&d).sqrt();
                if len < 1e-15 {
                    return vec![0.0];
                }
                // Formulación lineal: |dist| - r = 0.
                //
                // Considerada y rechazada formulación cuadrática
                // (cross²/len² - r²): aunque suave en cross=0, genera un
                // MÍNIMO LOCAL TRAMPA cuando la línea pasa por el centro
                // (gradiente cero ahí, solver queda atascado en dist=0).
                // La formulación lineal con abs() tiene cusp en cross=0
                // pero el sub-gradient ±1 permite que NR/LM salgan.
                let cross = d.x * f.y - d.y * f.x;
                vec![cross.abs() / len - radius]
            }

            Constraint::EqualLength {
                l1_p1,
                l1_p2,
                l2_p1,
                l2_p2,
            } => {
                let d1 = points[*l1_p2].co - points[*l1_p1].co;
                let d2 = points[*l2_p2].co - points[*l2_p1].co;
                // Reformulado: |d1| - |d2| = 0 (lineal)
                vec![d1.norm() - d2.norm()]
            }

            Constraint::Midpoint {
                p_idx,
                line_p1,
                line_p2,
            } => {
                let p = &points[*p_idx];
                let lp1 = &points[*line_p1];
                let lp2 = &points[*line_p2];
                let mid = (lp1.co + lp2.co) * 0.5;
                vec![p.x() - mid.x, p.y() - mid.y]
            }

            Constraint::Symmetric {
                p1_idx,
                p2_idx,
                line_p1,
                line_p2,
            } => {
                let p1 = &points[*p1_idx];
                let p2 = &points[*p2_idx];
                let lp1 = &points[*line_p1];
                let lp2 = &points[*line_p2];
                let mid = (p1.co + p2.co) * 0.5;
                let d = lp2.co - lp1.co;
                let to_mid = mid - lp1.co;
                // 1) Punto medio sobre la línea: cross(d, to_mid) = 0
                let cross = d.x * to_mid.y - d.y * to_mid.x;
                // 2) Segmento p1-p2 perpendicular a la línea: dot(d, p2-p1) = 0
                let seg = p2.co - p1.co;
                let dot = d.dot(&seg);
                vec![cross, dot]
            }

            Constraint::PointOnLine {
                p_idx,
                line_p1,
                line_p2,
            } => {
                let p = &points[*p_idx];
                let lp1 = &points[*line_p1];
                let lp2 = &points[*line_p2];
                let d = lp2.co - lp1.co;
                let f = p.co - lp1.co;
                vec![d.x * f.y - d.y * f.x]
            }

            Constraint::PointOnCircle {
                p_idx,
                center_idx,
                radius,
            } => {
                let p = &points[*p_idx];
                let c = &points[*center_idx];
                let d = p.co - c.co;
                // Reformulado: |p-c| - r = 0
                vec![d.norm() - radius]
            }

            Constraint::PointOnSpline {
                p_idx,
                spline_point_indices,
            } => {
                let p = &points[*p_idx];
                // Encontrar la proyección sobre el segmento más cercano
                // de la polilínea, devolver residual VECTORIAL (proj - p).
                //
                // Antes: residual = min_dist escalar (≥ 0). El solver no
                // sabe la dirección, sólo la magnitud → atascado en
                // mínimos locales con splines complejas.
                //
                // Ahora: residual 2D signed [proj.x - p.x, proj.y - p.y].
                // Gradiente bien definido en cada componente, convergencia
                // como un PointOnLine sobre el segmento más cercano.
                let mut best_d = f64::INFINITY;
                let mut best_proj = p.co;
                for i in 0..spline_point_indices.len().saturating_sub(1) {
                    let a = &points[spline_point_indices[i]];
                    let b = &points[spline_point_indices[i + 1]];
                    let d = b.co - a.co;
                    let len_sq = d.dot(&d);
                    if len_sq < 1e-15 {
                        continue;
                    }
                    let t = ((p.co - a.co).dot(&d) / len_sq).clamp(0.0, 1.0);
                    let proj = a.co + d * t;
                    let dist_sq = (p.co - proj).dot(&(p.co - proj));
                    if dist_sq < best_d {
                        best_d = dist_sq;
                        best_proj = proj;
                    }
                }
                vec![best_proj.x - p.x(), best_proj.y - p.y()]
            }

            Constraint::Radius {
                center_idx,
                p_on_circle,
                radius,
            } => {
                let c = &points[*center_idx];
                let p = &points[*p_on_circle];
                let d = p.co - c.co;
                // Reformulado: |p-c| - r = 0
                vec![d.norm() - radius]
            }
        }
    }

    /// Entradas del Jacobiano: (fila_local, índice_punto, df/dx, df/dy)
    pub fn jacobian_entries(&self, points: &[Point2]) -> Vec<(usize, usize, f64, f64)> {
        match self {
            Constraint::Coincident { p1_idx, p2_idx } => {
                vec![
                    // df0/dp1 = (1, 0), df0/dp2 = (-1, 0)
                    (0, *p1_idx, 1.0, 0.0),
                    (0, *p2_idx, -1.0, 0.0),
                    // df1/dp1 = (0, 1), df1/dp2 = (0, -1)
                    (1, *p1_idx, 0.0, 1.0),
                    (1, *p2_idx, 0.0, -1.0),
                ]
            }

            Constraint::Distance {
                p1_idx, p2_idx, ..
            } => {
                let p1 = &points[*p1_idx];
                let p2 = &points[*p2_idx];
                let dx = p1.x() - p2.x();
                let dy = p1.y() - p2.y();
                let len = (dx * dx + dy * dy).sqrt();
                if len < 1e-15 {
                    return self.jacobian_numerical(points);
                }
                // f = |p1-p2| - d  →  df/dp1 = (dx/len, dy/len)
                let inv = 1.0 / len;
                vec![
                    (0, *p1_idx, dx * inv, dy * inv),
                    (0, *p2_idx, -dx * inv, -dy * inv),
                ]
            }

            Constraint::HorizontalDist {
                p1_idx, p2_idx, ..
            } => {
                vec![(0, *p1_idx, 1.0, 0.0), (0, *p2_idx, -1.0, 0.0)]
            }

            Constraint::VerticalDist {
                p1_idx, p2_idx, ..
            } => {
                vec![(0, *p1_idx, 0.0, 1.0), (0, *p2_idx, 0.0, -1.0)]
            }

            Constraint::Fixed { p_idx, .. } => {
                vec![(0, *p_idx, 1.0, 0.0), (1, *p_idx, 0.0, 1.0)]
            }

            Constraint::Horizontal { p1_idx, p2_idx } => {
                vec![(0, *p1_idx, 0.0, 1.0), (0, *p2_idx, 0.0, -1.0)]
            }

            Constraint::Vertical { p1_idx, p2_idx } => {
                vec![(0, *p1_idx, 1.0, 0.0), (0, *p2_idx, -1.0, 0.0)]
            }

            Constraint::Perpendicular {
                l1_p1,
                l1_p2,
                l2_p1,
                l2_p2,
            } => {
                let d1 = points[*l1_p2].co - points[*l1_p1].co;
                let d2 = points[*l2_p2].co - points[*l2_p1].co;
                // f = d1·d2 = d1x*d2x + d1y*d2y
                // d1 = l1_p2 - l1_p1, d2 = l2_p2 - l2_p1
                // df/dl1_p1 = (-d2x, -d2y)
                // df/dl1_p2 = (d2x, d2y)
                // df/dl2_p1 = (-d1x, -d1y)
                // df/dl2_p2 = (d1x, d1y)
                vec![
                    (0, *l1_p1, -d2.x, -d2.y),
                    (0, *l1_p2, d2.x, d2.y),
                    (0, *l2_p1, -d1.x, -d1.y),
                    (0, *l2_p2, d1.x, d1.y),
                ]
            }

            Constraint::Parallel {
                l1_p1,
                l1_p2,
                l2_p1,
                l2_p2,
            } => {
                let d1 = points[*l1_p2].co - points[*l1_p1].co;
                let d2 = points[*l2_p2].co - points[*l2_p1].co;
                // f = d1x*d2y - d1y*d2x
                // df/dl1_p1 = (-d2y, d2x)
                // df/dl1_p2 = (d2y, -d2x)
                // df/dl2_p1 = (d1y, -d1x)
                // df/dl2_p2 = (-d1y, d1x)
                vec![
                    (0, *l1_p1, -d2.y, d2.x),
                    (0, *l1_p2, d2.y, -d2.x),
                    (0, *l2_p1, d1.y, -d1.x),
                    (0, *l2_p2, -d1.y, d1.x),
                ]
            }

            Constraint::Angle {
                ..
            } => {
                // f = atan2(cross, dot) - angle
                // Usar diferenciación numérica para atan2 (analítico complejo)
                self.jacobian_numerical(points)
            }

            Constraint::TangentLineCircle {
                line_p1,
                line_p2,
                center_idx,
                radius: _,
            } => {
                let lp1 = &points[*line_p1];
                let lp2 = &points[*line_p2];
                let c = &points[*center_idx];
                let d = lp2.co - lp1.co;
                let f = lp1.co - c.co;
                let len_sq = d.dot(&d);
                if len_sq < 1e-15 {
                    return vec![];
                }
                // f = (dx*fy - dy*fx)² / |d|² - r²
                let _cross = d.x * f.y - d.y * f.x;
                let _inv_len_sq = 1.0 / len_sq;
                // Numérico por complejidad de las derivadas con cociente
                self.jacobian_numerical(points)
            }

            Constraint::EqualLength {
                l1_p1,
                l1_p2,
                l2_p1,
                l2_p2,
            } => {
                let d1 = points[*l1_p2].co - points[*l1_p1].co;
                let d2 = points[*l2_p2].co - points[*l2_p1].co;
                let len1 = d1.norm();
                let len2 = d2.norm();
                if len1 < 1e-15 || len2 < 1e-15 {
                    return self.jacobian_numerical(points);
                }
                // f = |d1| - |d2|  →  df/dl1_p1 = (-d1x/|d1|, -d1y/|d1|)
                let inv1 = 1.0 / len1;
                let inv2 = 1.0 / len2;
                vec![
                    (0, *l1_p1, -d1.x * inv1, -d1.y * inv1),
                    (0, *l1_p2, d1.x * inv1, d1.y * inv1),
                    (0, *l2_p1, d2.x * inv2, d2.y * inv2),
                    (0, *l2_p2, -d2.x * inv2, -d2.y * inv2),
                ]
            }

            Constraint::Midpoint {
                p_idx,
                line_p1,
                line_p2,
            } => {
                // f0 = px - (l1x + l2x)/2, f1 = py - (l1y + l2y)/2
                vec![
                    (0, *p_idx, 1.0, 0.0),
                    (0, *line_p1, -0.5, 0.0),
                    (0, *line_p2, -0.5, 0.0),
                    (1, *p_idx, 0.0, 1.0),
                    (1, *line_p1, 0.0, -0.5),
                    (1, *line_p2, 0.0, -0.5),
                ]
            }

            Constraint::Symmetric {
                p1_idx: _,
                p2_idx: _,
                line_p1: _,
                line_p2: _,
            } => {
                // Complejo — usar numérico
                self.jacobian_numerical(points)
            }

            Constraint::PointOnLine {
                p_idx,
                line_p1,
                line_p2,
            } => {
                let p = &points[*p_idx];
                let lp1 = &points[*line_p1];
                let lp2 = &points[*line_p2];
                let d = lp2.co - lp1.co;
                let f = p.co - lp1.co;
                // f = dx*fy - dy*fx  donde d = lp2-lp1, f = p-lp1
                // df/dp = (dy_line, -dx_line) → wait:
                // f = dx*(py-l1y) - dy*(px-l1x)
                // df/dpx = -dy, df/dpy = dx
                // df/dl1x = dy, df/dl1y = -(dx) → wait, recalcular:
                // f = (l2x-l1x)*(py-l1y) - (l2y-l1y)*(px-l1x)
                // df/dpx = -(l2y-l1y) = -d.y
                // df/dpy = (l2x-l1x) = d.x
                // df/dl1x = -(py-l1y) + (px-l1x)... mejor: expandir
                // f = d.x*py - d.x*l1y - d.y*px + d.y*l1x
                // df/dl1x = d.y - ... parcial respecto a l1x:
                //   d.x = l2x - l1x → dd.x/dl1x = -1
                //   f = d.x*(py-l1y) - d.y*(px-l1x)
                //   df/dl1x = -1*(py-l1y) - d.y*(-1) = -(py-l1y) + d.y = -f.y + d.y
                //   df/dl1y = d.x*(-1) - (-1)*(px-l1x) = -d.x + f.x
                //   df/dl2x = 1*(py-l1y) = f.y
                //   df/dl2y = -1*(px-l1x) = -f.x
                // Simplificado:
                vec![
                    (0, *p_idx, -d.y, d.x),
                    (0, *line_p1, -f.y + d.y, -d.x + f.x),
                    (0, *line_p2, f.y, -f.x),
                ]
            }

            Constraint::PointOnCircle {
                p_idx,
                center_idx,
                ..
            } => {
                let p = &points[*p_idx];
                let c = &points[*center_idx];
                let dx = p.x() - c.x();
                let dy = p.y() - c.y();
                let len = (dx * dx + dy * dy).sqrt();
                if len < 1e-15 {
                    return self.jacobian_numerical(points);
                }
                // f = |p-c| - r  →  df/dp = (dx/len, dy/len)
                let inv = 1.0 / len;
                vec![
                    (0, *p_idx, dx * inv, dy * inv),
                    (0, *center_idx, -dx * inv, -dy * inv),
                ]
            }

            Constraint::Radius {
                center_idx,
                p_on_circle,
                ..
            } => {
                let c = &points[*center_idx];
                let p = &points[*p_on_circle];
                let dx = p.x() - c.x();
                let dy = p.y() - c.y();
                let len = (dx * dx + dy * dy).sqrt();
                if len < 1e-15 {
                    return self.jacobian_numerical(points);
                }
                // f = |p-c| - r
                let inv = 1.0 / len;
                vec![
                    (0, *center_idx, -dx * inv, -dy * inv),
                    (0, *p_on_circle, dx * inv, dy * inv),
                ]
            }

            Constraint::PointOnSpline { .. } => {
                self.jacobian_numerical(points)
            }
        }
    }

    /// Jacobiano numérico (fallback para constraints complejos)
    fn jacobian_numerical(&self, points: &[Point2]) -> Vec<(usize, usize, f64, f64)> {
        let eps = 1e-8;
        let indices = self.point_indices();
        let base = self.residuals(points);
        let mut entries = Vec::new();

        for &pi in &indices {
            let mut pts_dx = points.to_vec();
            pts_dx[pi].co.x += eps;
            let res_dx = self.residuals(&pts_dx);

            let mut pts_dy = points.to_vec();
            pts_dy[pi].co.y += eps;
            let res_dy = self.residuals(&pts_dy);

            for (row, _) in base.iter().enumerate() {
                let dfdx = (res_dx[row] - base[row]) / eps;
                let dfdy = (res_dy[row] - base[row]) / eps;
                if dfdx.abs() > 1e-12 || dfdy.abs() > 1e-12 {
                    entries.push((row, pi, dfdx, dfdy));
                }
            }
        }

        entries
    }
}
