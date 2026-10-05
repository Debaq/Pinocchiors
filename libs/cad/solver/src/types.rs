use nalgebra::Vector2;

/// Punto 2D — 2 grados de libertad (x, y)
#[derive(Debug, Clone, Copy)]
pub struct Point2 {
    pub co: Vector2<f64>,
}

impl Point2 {
    pub fn new(x: f64, y: f64) -> Self {
        Self {
            co: Vector2::new(x, y),
        }
    }

    pub fn x(&self) -> f64 {
        self.co.x
    }

    pub fn y(&self) -> f64 {
        self.co.y
    }
}

/// Línea 2D definida por dos puntos (referenciados por índice)
#[derive(Debug, Clone, Copy)]
pub struct Line2 {
    pub p1_idx: usize,
    pub p2_idx: usize,
}

impl Line2 {
    pub fn new(p1_idx: usize, p2_idx: usize) -> Self {
        Self { p1_idx, p2_idx }
    }
}

/// Círculo 2D — centro (referenciado por índice) + radio
#[derive(Debug, Clone, Copy)]
pub struct Circle2 {
    pub center_idx: usize,
    pub radius: f64,
}

impl Circle2 {
    pub fn new(center_idx: usize, radius: f64) -> Self {
        Self { center_idx, radius }
    }
}

/// Arco 2D — centro + radio + ángulos inicio/fin
#[derive(Debug, Clone, Copy)]
pub struct Arc2 {
    pub center_idx: usize,
    pub start_idx: usize,
    pub end_idx: usize,
    pub radius: f64,
}

impl Arc2 {
    pub fn new(center_idx: usize, start_idx: usize, end_idx: usize, radius: f64) -> Self {
        Self {
            center_idx,
            start_idx,
            end_idx,
            radius,
        }
    }
}

/// Entidad geométrica en un sketch
#[derive(Debug, Clone)]
pub enum Entity {
    Point(Point2),
    Line(Line2),
    Circle(Circle2),
    Arc(Arc2),
}

/// Resultado del solver
#[derive(Debug, Clone)]
pub struct SolveResult {
    pub status: SolveStatus,
    pub dof: i32,
    pub iterations: usize,
    pub residual: f64,
    pub points: Vec<Point2>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolveStatus {
    Converged,
    NotConverged,
    OverConstrained,
    UnderConstrained,
}
