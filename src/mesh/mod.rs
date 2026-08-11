pub mod build;
pub mod cut;
pub mod delaunay;
pub mod half_edge;
pub mod obj_loader;
pub mod stats;
pub mod surface;
pub mod torus;
pub mod uv;
pub mod vertex;
/// 三角形索引 (v0, v1, v2)。
pub type TriIndex = (usize, usize, usize);
/// 四边形索引 (v00, v10, v11, v01)。
pub type QuadIndex = (usize, usize, usize, usize);
/// 按补片 (i, j) 分组的三角形列表。
pub type PatchTriMap = std::collections::BTreeMap<(usize, usize), Vec<TriIndex>>;
