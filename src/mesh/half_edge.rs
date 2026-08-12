use glam::{Vec2, Vec3};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VertexId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HalfEdgeId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FaceId(pub usize);

pub struct FaceHalfEdgeIter<'a> {
    mesh: &'a HalfEdgeMesh,
    start: HalfEdgeId,
    current: HalfEdgeId,
    first: bool,
}

impl<'a> Iterator for FaceHalfEdgeIter<'a> {
    type Item = HalfEdgeId;

    fn next(&mut self) -> Option<Self::Item> {
        if self.first {
            self.first = false;
            Some(self.current)
        } else {
            self.current = self.mesh.half_edges[self.current.0].next;
            if self.current == self.start {
                None
            } else {
                Some(self.current)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct HEVertex {
    pub position: Vec3,
    pub uv: Vec2,
    pub outgoing: HalfEdgeId,
}

#[derive(Debug, Clone)]
pub struct HalfEdge {
    pub origin: VertexId,
    pub twin: HalfEdgeId,
    pub next: HalfEdgeId,
    pub prev: HalfEdgeId,
    pub face: FaceId,
    /// 该无向边位于某条切割曲线上（切割产生的 diag 边、或沿切割线
    /// 对齐的原边）。连通块 flood-fill 视为不连通——ByRegion 拓扑分块依据。
    pub cut: bool,
}

#[derive(Debug, Clone)]
pub struct HEFace {
    pub half_edge: HalfEdgeId,
    pub valid: bool,
    pub patch_index: Option<(usize, usize)>,
    /// Topological connected-component ID assigned after cutting.
    /// Faces reachable via twin half-edges (across un-cut edges) share the same ID.
    /// Used by `ColorMode::ByRegion` for correct patch coloring.
    pub component_id: Option<usize>,
}

// ── Mesh container ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct HalfEdgeMesh {
    pub vertices: Vec<HEVertex>,
    pub half_edges: Vec<HalfEdge>,
    pub faces: Vec<HEFace>,
}

impl Default for HalfEdgeMesh {
    fn default() -> Self {
        Self::new()
    }
}

impl HalfEdgeMesh {
    pub fn new() -> Self {
        HalfEdgeMesh {
            vertices: Vec::new(),
            half_edges: Vec::new(),
            faces: Vec::new(),
        }
    }

    pub fn num_valid_faces(&self) -> usize {
        self.faces.iter().filter(|f| f.valid).count()
    }

    // ── Build from triangle soup ───────────────────────────────────

    /// Build half-edge mesh from triangle vertex data.
    /// `tri_verts`: for each triangle, the three (vertex_index, uv) entries.
    /// `positions`: 3D position for each vertex index.
    ///
    /// Duplicate vertices (same position within epsilon) are NOT automatically
    /// merged; the caller is expected to provide a deduplicated vertex set.
    pub fn from_triangles(
        positions: &[Vec3],
        uvs: &[Vec2],
        face_triplets: &[(usize, usize, usize)],
    ) -> Self {
        let mut mesh = HalfEdgeMesh::new();

        // Create vertices
        for (i, &pos) in positions.iter().enumerate() {
            mesh.vertices.push(HEVertex {
                position: pos,
                uv: uvs.get(i).copied().unwrap_or(Vec2::ZERO),
                outgoing: HalfEdgeId(usize::MAX), // placeholder
            });
        }

        // Map: (v_a, v_b) -> HalfEdgeId, where v_a < v_b for the undirected edge
        let mut edge_map: HashMap<(usize, usize), HalfEdgeId> = HashMap::new();

        for &(v0, v1, v2) in face_triplets {
            let face_id = FaceId(mesh.faces.len());

            // Create three half-edges for this face
            let he0 = HalfEdgeId(mesh.half_edges.len());
            let he1 = HalfEdgeId(mesh.half_edges.len() + 1);
            let he2 = HalfEdgeId(mesh.half_edges.len() + 2);

            // he0: v0 -> v1
            mesh.half_edges.push(HalfEdge {
                origin: VertexId(v0),
                twin: HalfEdgeId(usize::MAX), // filled below
                next: he1,
                prev: he2,
                face: face_id,
                cut: false,
            });
            // he1: v1 -> v2
            mesh.half_edges.push(HalfEdge {
                origin: VertexId(v1),
                twin: HalfEdgeId(usize::MAX),
                next: he2,
                prev: he0,
                face: face_id,
                cut: false,
            });
            // he2: v2 -> v0
            mesh.half_edges.push(HalfEdge {
                origin: VertexId(v2),
                twin: HalfEdgeId(usize::MAX),
                next: he0,
                prev: he1,
                face: face_id,
                cut: false,
            });

            mesh.faces.push(HEFace {
                half_edge: he0,
                valid: true,
                patch_index: None,
                component_id: None,
            });

            // Set outgoing half-edge for each vertex
            mesh.vertices[v0].outgoing = he0;
            mesh.vertices[v1].outgoing = he1;
            mesh.vertices[v2].outgoing = he2;

            // Link twins: for each directed edge, look for its reverse
            for &(a, b, he) in &[(v0, v1, he0), (v1, v2, he1), (v2, v0, he2)] {
                let key = if a < b { (a, b) } else { (b, a) };
                if let Some(&existing) = edge_map.get(&key) {
                    // Twin found
                    mesh.half_edges[he.0].twin = existing;
                    mesh.half_edges[existing.0].twin = he;
                } else {
                    edge_map.insert(key, he);
                }
            }
        }

        mesh
    }

    /// Build half-edge mesh from quad vertex data.
    /// Each quad is (v0, v1, v2, v3) in CCW order.
    pub fn from_quads(
        positions: &[Vec3],
        uvs: &[Vec2],
        face_quads: &[(usize, usize, usize, usize)],
    ) -> Self {
        let mut mesh = HalfEdgeMesh::new();

        for (i, &pos) in positions.iter().enumerate() {
            mesh.vertices.push(HEVertex {
                position: pos,
                uv: uvs.get(i).copied().unwrap_or(Vec2::ZERO),
                outgoing: HalfEdgeId(usize::MAX),
            });
        }

        let mut edge_map: HashMap<(usize, usize), HalfEdgeId> = HashMap::new();

        for &(v0, v1, v2, v3) in face_quads {
            let face_id = FaceId(mesh.faces.len());

            let he0 = HalfEdgeId(mesh.half_edges.len());
            let he1 = HalfEdgeId(mesh.half_edges.len() + 1);
            let he2 = HalfEdgeId(mesh.half_edges.len() + 2);
            let he3 = HalfEdgeId(mesh.half_edges.len() + 3);

            mesh.half_edges.push(HalfEdge {
                origin: VertexId(v0),
                twin: HalfEdgeId(usize::MAX),
                next: he1,
                prev: he3,
                face: face_id,
                cut: false,
            });
            mesh.half_edges.push(HalfEdge {
                origin: VertexId(v1),
                twin: HalfEdgeId(usize::MAX),
                next: he2,
                prev: he0,
                face: face_id,
                cut: false,
            });
            mesh.half_edges.push(HalfEdge {
                origin: VertexId(v2),
                twin: HalfEdgeId(usize::MAX),
                next: he3,
                prev: he1,
                face: face_id,
                cut: false,
            });
            mesh.half_edges.push(HalfEdge {
                origin: VertexId(v3),
                twin: HalfEdgeId(usize::MAX),
                next: he0,
                prev: he2,
                face: face_id,
                cut: false,
            });

            mesh.faces.push(HEFace {
                half_edge: he0,
                valid: true,
                patch_index: None,
                component_id: None,
            });

            mesh.vertices[v0].outgoing = he0;
            mesh.vertices[v1].outgoing = he1;
            mesh.vertices[v2].outgoing = he2;
            mesh.vertices[v3].outgoing = he3;

            for &(a, b, he) in &[(v0, v1, he0), (v1, v2, he1), (v2, v3, he2), (v3, v0, he3)] {
                let key = if a < b { (a, b) } else { (b, a) };
                if let Some(&existing) = edge_map.get(&key) {
                    mesh.half_edges[he.0].twin = existing;
                    mesh.half_edges[existing.0].twin = he;
                } else {
                    edge_map.insert(key, he);
                }
            }
        }

        mesh
    }

    pub fn face_half_edges(&self, face: FaceId) -> Vec<HalfEdgeId> {
        let start = self.faces[face.0].half_edge;
        let mut result = vec![start];
        // Safety cap: a malformed half-edge cycle that never returns to `start`
        // would otherwise loop forever. The bound is generous — no valid face
        // can have more half-edges than the whole mesh.
        let cap = self.half_edges.len() + 1;
        let mut current = self.half_edges[start.0].next;
        while current != start && result.len() < cap {
            result.push(current);
            current = self.half_edges[current.0].next;
        }
        result
    }

    pub fn face_half_edges_iter(&self, face: FaceId) -> FaceHalfEdgeIter<'_> {
        let start = self.faces[face.0].half_edge;
        FaceHalfEdgeIter {
            mesh: self,
            start,
            current: start,
            first: true,
        }
    }

    pub fn face_vertex(&self, he: HalfEdgeId) -> usize {
        self.half_edges[he.0].origin.0
    }

    // ── Vertex utilities ────────────────────────────────────────────

    /// Add a new vertex, returns its id
    pub fn add_vertex(&mut self, position: Vec3, uv: Vec2) -> VertexId {
        let id = VertexId(self.vertices.len());
        self.vertices.push(HEVertex {
            position,
            uv,
            outgoing: HalfEdgeId(usize::MAX),
        });
        id
    }

    // ── Split operations ───────────────────────────────────────────

    /// Split a half-edge at the given position.
    ///
    /// Creates a new vertex and splits the half-edge (and its twin) in two.
    /// Returns the new vertex ID.
    pub fn split_edge(&mut self, he_id: HalfEdgeId, position: Vec3, uv: Vec2) -> VertexId {
        let v_new = self.add_vertex(position, uv);

        let twin_id = self.half_edges[he_id.0].twin;

        // ── Split the forward half-edge (he) ─────────────────────
        // he: A → B  becomes  he: A → V_new
        // he_new: V_new → B
        let old_next = self.half_edges[he_id.0].next;
        let he_new_id = HalfEdgeId(self.half_edges.len());

        self.half_edges.push(HalfEdge {
            origin: v_new,
            twin: HalfEdgeId(usize::MAX), // filled below
            next: old_next,
            prev: he_id,
            face: self.half_edges[he_id.0].face,
            cut: self.half_edges[he_id.0].cut,
        });

        // Update he
        self.half_edges[he_id.0].next = he_new_id;

        // Update old_next's prev to point to he_new
        self.half_edges[old_next.0].prev = he_new_id;

        // ── Split the twin half-edge (reverse direction) ─────────
        // twin: B → A  becomes  twin: B → V_new
        // twin_new: V_new → A
        if twin_id.0 != usize::MAX {
            let old_twin_next = self.half_edges[twin_id.0].next;
            let twin_new_id = HalfEdgeId(self.half_edges.len());

            // Twin new: V_new → A (was the second half of the twin edge)
            self.half_edges.push(HalfEdge {
                origin: v_new,
                twin: he_new_id, // twin of twin_new is he_new
                next: old_twin_next,
                prev: twin_id,
                face: self.half_edges[twin_id.0].face,
                cut: self.half_edges[twin_id.0].cut,
            });

            // Update twin: B → V_new
            self.half_edges[twin_id.0].next = twin_new_id;
            self.half_edges[old_twin_next.0].prev = twin_new_id;

            // Link he_new <-> twin_new
            self.half_edges[he_new_id.0].twin = twin_id;
            self.half_edges[twin_id.0].twin = he_new_id;

            // Actually he_new.twin = twin (modified), and twin.twin = he_new
            // Wait: twin was B→A, now B→V_new. he_new is V_new→B.
            // They are NOT twins! he_new goes V_new→B, twin goes B→V_new. They ARE twins.
            // Correct: he_new.twin = twin (the modified twin)
            // And: twin_new.twin = he (the modified original he)
            self.half_edges[twin_new_id.0].twin = he_id;
            self.half_edges[he_id.0].twin = twin_new_id;
        } else {
            // No twin (boundary edge)
            self.half_edges[he_new_id.0].twin = HalfEdgeId(usize::MAX);
        }

        // Set outgoing for the new vertex
        self.vertices[v_new.0].outgoing = he_new_id;

        v_new
    }

    /// Insert an interior vertex `pos` into `face` and fan-triangulate the face
    /// around it — i.e. replace `face` (a polygon) with a triangle fan rooted at
    /// the new vertex. Returns the new vertex id and, for each original boundary
    /// vertex `v_i`, the half-edge `P -> v_i` (so the caller can mark chord
    /// segments that pass through `P` as `cut` barriers).
    ///
    /// Used when two cut curves cross *inside* a face: the crossing point must be
    /// a real mesh vertex so that the four incident faces are separated by the
    /// two curves (a transverse crossing), instead of the two curves meeting at a
    /// vertex as one series-connected loop.
    ///
    /// All new fan half-edges are created with `cut = false` (the caller marks
    /// the chord sub-segments). The original boundary half-edges keep their twins
    /// (neighbouring faces are untouched).
    pub fn insert_interior_vertex_fan(
        &mut self,
        face: FaceId,
        pos: Vec3,
        uv: Vec2,
    ) -> (VertexId, Vec<HalfEdgeId>) {
        let hes = self.face_half_edges(face);
        let m = hes.len();
        let p = self.add_vertex(pos, uv);
        if m < 3 {
            return (p, Vec::new());
        }
        let origs: Vec<VertexId> = hes.iter().map(|&h| self.half_edges[h.0].origin).collect();

        // Interior half-edge pairs: pe_i = P->v_i (in T_i), ie_i = v_i->P (in T_{i-1}).
        // pe_i and ie_i are twins of each other.
        let mut pe: Vec<HalfEdgeId> = Vec::with_capacity(m);
        let mut ie: Vec<HalfEdgeId> = Vec::with_capacity(m);
        for _ in 0..m {
            let id = HalfEdgeId(self.half_edges.len());
            pe.push(id);
            self.half_edges.push(HalfEdge {
                origin: p,
                twin: HalfEdgeId(usize::MAX),
                next: HalfEdgeId(usize::MAX),
                prev: HalfEdgeId(usize::MAX),
                face: FaceId(usize::MAX),
                cut: false,
            });
        }
        for i in 0..m {
            let id = HalfEdgeId(self.half_edges.len());
            ie.push(id);
            self.half_edges.push(HalfEdge {
                origin: origs[i],
                twin: pe[i],
                next: HalfEdgeId(usize::MAX),
                prev: HalfEdgeId(usize::MAX),
                face: FaceId(usize::MAX),
                cut: false,
            });
            self.half_edges[pe[i].0].twin = id;
        }

        // Reuse `face` as T_0 and create m-1 new faces T_1..T_{m-1}.
        // T_i = (P, v_i, v_{i+1}); cycle: pe_i -> h_i -> ie_{i+1} -> pe_i.
        for i in 0..m {
            let fi = if i == 0 {
                face
            } else {
                self.faces.push(HEFace {
                    half_edge: pe[i],
                    valid: true,
                    patch_index: self.faces[face.0].patch_index,
                    component_id: None,
                });
                FaceId(self.faces.len() - 1)
            };
            if i == 0 {
                self.faces[face.0].half_edge = pe[i];
            }
            self.half_edges[pe[i].0].face = fi;
            self.half_edges[hes[i].0].face = fi; // h_i = v_i -> v_{i+1} (reused)
            self.half_edges[ie[(i + 1) % m].0].face = fi;
        }

        for i in 0..m {
            let h_i = hes[i];
            let ie_next = ie[(i + 1) % m];
            self.half_edges[pe[i].0].next = h_i;
            self.half_edges[pe[i].0].prev = ie_next;
            self.half_edges[h_i.0].next = ie_next;
            self.half_edges[h_i.0].prev = pe[i];
            self.half_edges[ie_next.0].next = pe[i];
            self.half_edges[ie_next.0].prev = h_i;
        }

        self.vertices[p.0].outgoing = pe[0];
        for i in 0..m {
            self.vertices[origs[i].0].outgoing = hes[i];
        }

        (p, pe)
    }

    /// Split a face along a diagonal between two of its boundary vertices.
    ///
    /// `v_a` and `v_b` must be distinct vertices on the boundary of `face`.
    /// `v_a` must come before `v_b` in the CCW cycle.
    /// `cut_edge` 标记该对角线是否位于切割曲线上（切割操作产生 → true；
    /// fan 三角化/其他细分 → false）。cut 边在连通块 flood-fill 中视为不连通。
    pub fn split_face(
        &mut self,
        face: FaceId,
        v_a: VertexId,
        v_b: VertexId,
        cut_edge: bool,
    ) -> FaceId {
        if v_a == v_b {
            return face; // degenerate
        }

        let hes = self.face_half_edges(face);

        // Find half-edges originating at v_a and v_b
        let mut he_a = HalfEdgeId(usize::MAX);
        let mut he_b = HalfEdgeId(usize::MAX);
        for &he in &hes {
            let orig = self.half_edges[he.0].origin;
            if orig == v_a {
                he_a = he;
            }
            if orig == v_b {
                he_b = he;
            }
        }

        if he_a.0 == usize::MAX || he_b.0 == usize::MAX {
            return face; // vertex not found
        }

        // Don't split if v_a and v_b are adjacent (already connected by an edge)
        if self.half_edges[he_a.0].next == he_b || self.half_edges[he_b.0].next == he_a {
            return face;
        }

        // Find the half-edge before he_a (prev_he_a) and before he_b (prev_he_b)
        let mut prev_he_a = HalfEdgeId(usize::MAX);
        let mut prev_he_b = HalfEdgeId(usize::MAX);
        for &he in &hes {
            if self.half_edges[he.0].next == he_a {
                prev_he_a = he;
            }
            if self.half_edges[he.0].next == he_b {
                prev_he_b = he;
            }
        }

        // Create two new half-edges for the diagonal
        let diag_ab = HalfEdgeId(self.half_edges.len());
        let diag_ba = HalfEdgeId(self.half_edges.len() + 1);
        let new_face_id = FaceId(self.faces.len());

        self.faces.push(HEFace {
            half_edge: diag_ba,
            valid: true,
            patch_index: self.faces[face.0].patch_index,
            component_id: None,
        });

        // diag_ab: v_a → v_b
        // Its next is he_b (continue CCW from v_b along original boundary)
        // Its prev is prev_he_a (the edge just before where we cut from)
        self.half_edges.push(HalfEdge {
            origin: v_a,
            twin: diag_ba,
            next: he_b,
            prev: prev_he_a,
            face,
            cut: cut_edge,
        });

        // diag_ba: v_b → v_a
        self.half_edges.push(HalfEdge {
            origin: v_b,
            twin: diag_ab,
            next: he_a,
            prev: prev_he_b,
            face: new_face_id,
            cut: cut_edge,
        });

        // Rewire surrounding half-edges
        self.half_edges[prev_he_a.0].next = diag_ab;
        self.half_edges[he_b.0].prev = diag_ab;
        self.half_edges[prev_he_b.0].next = diag_ba;
        self.half_edges[he_a.0].prev = diag_ba;

        // Reassign faces for half-edges in each cycle.
        // Original face F = diag_ab → he_b → ... → prev_he_a → diag_ab
        let mut cur = he_b;
        loop {
            self.half_edges[cur.0].face = face;
            if cur == prev_he_a {
                break;
            }
            cur = self.half_edges[cur.0].next;
            if cur == he_b {
                break;
            } // safety
        }

        // New face G = diag_ba → he_a → ... → prev_he_b → diag_ba
        let mut cur = he_a;
        loop {
            self.half_edges[cur.0].face = new_face_id;
            if cur == prev_he_b {
                break;
            }
            cur = self.half_edges[cur.0].next;
            if cur == he_a {
                break;
            } // safety
        }

        self.faces[face.0].half_edge = diag_ab;

        new_face_id
    }

    pub fn compute_vertex_normals(&self) -> Vec<Vec3> {
        let mut normals = vec![Vec3::ZERO; self.vertices.len()];
        for (fi, face) in self.faces.iter().enumerate() {
            if !face.valid {
                continue;
            }
            let mut iter = self.face_half_edges_iter(FaceId(fi));
            let first_he = match iter.next() {
                Some(he) => he,
                None => continue,
            };
            let p0 = self.vertices[self.half_edges[first_he.0].origin.0].position;
            let mut face_normal = Vec3::ZERO;
            let mut prev_he = first_he;
            let mut count = 1u32;
            for he in iter {
                let pi = self.vertices[self.half_edges[prev_he.0].origin.0].position;
                let pj = self.vertices[self.half_edges[he.0].origin.0].position;
                face_normal += (pi - p0).cross(pj - p0);
                prev_he = he;
                count += 1;
            }
            if count < 3 {
                continue;
            }
            for he in self.face_half_edges_iter(FaceId(fi)) {
                let vi = self.half_edges[he.0].origin.0;
                normals[vi] += face_normal;
            }
        }
        for (vi, vertex) in self.vertices.iter().enumerate() {
            if normals[vi].length_squared() > 1e-12 {
                normals[vi] = normals[vi].normalize();
            } else {
                normals[vi] = Vec3::Z;
            }
            let view_dir = vertex.position - Vec3::ZERO;
            if normals[vi].dot(view_dir) < 0.0 {
                normals[vi] = -normals[vi];
            }
        }
        normals
    }

    // ── Edge flip ──────────────────────────────────────────────────

    /// Flip an interior edge shared by two triangles.
    ///
    /// Before: triangles (a, b, c) and (b, a, d) sharing edge a→b
    /// After:  triangles (a, d, c) and (b, c, d) sharing edge d→c
    ///
    /// Returns true if the flip was performed successfully.
    pub fn flip_edge(&mut self, he_id: HalfEdgeId) -> bool {
        let twin_id = self.half_edges[he_id.0].twin;

        // Must have a twin (interior edge)
        if twin_id.0 == usize::MAX {
            return false;
        }

        // Both faces must be triangles
        let face1 = self.half_edges[he_id.0].face;
        let face2 = self.half_edges[twin_id.0].face;
        if self.face_half_edges(face1).len() != 3 {
            return false;
        }
        if self.face_half_edges(face2).len() != 3 {
            return false;
        }

        // Get the 4 vertices and 6 half-edges
        //   he0 = he_id:     a→b    he1 = he_next:  b→c
        //   he2 = he_prev:   c→a    he3 = twin:     b→a
        //   he4 = twin_next: a→d    he5 = twin_prev: d→b
        let a = self.half_edges[he_id.0].origin;
        let he_next = self.half_edges[he_id.0].next;
        let he_prev = self.half_edges[he_id.0].prev;
        let b = self.half_edges[twin_id.0].origin;
        let twin_next = self.half_edges[twin_id.0].next;
        let twin_prev = self.half_edges[twin_id.0].prev;
        let c = self.half_edges[he_prev.0].origin;
        let d = self.half_edges[twin_prev.0].origin;

        // c and d must be different vertices
        if c == d {
            return false;
        }

        // c and d must not already be connected (would create non-manifold edge)
        if self.are_vertices_connected(c, d) {
            return false;
        }

        // Convexity check: use UV coordinates for topological flips.
        // On a curved surface (torus), 3D-based convexity fails because
        // adjacent faces have different normals. UV-based check correctly
        // detects whether the quad a-c-b-d is convex in the parametric domain.
        //
        // Original triangles (both CCW):
        //   F1: a→b→c  (shared edge a→b, c is to the LEFT of a→b)
        //   F2: b→a→d  (shared edge b→a, d is to the LEFT of b→a, i.e. RIGHT of a→b)
        //
        // The quadrilateral is a-c-b-d. The flip is valid if diagonal c→d
        // lies inside the quad, which is equivalent to:
        //   - c is on the LEFT of the new edge a→d (or equivalently, a is on the LEFT of d→c)
        //   - d is on the LEFT of the new edge c→b (or equivalently, b is on the LEFT of d→c)
        //
        // In 2D UV space, cross product sign tells us which side a point is on:
        //   cross(uv1 - uv0, uv2 - uv0) > 0  →  uv2 is LEFT of uv0→uv1
        let uva = self.vertices[a.0].uv;
        let uvb = self.vertices[b.0].uv;
        let uvc = self.vertices[c.0].uv;
        let uvd = self.vertices[d.0].uv;

        fn cross2d(a: Vec2, b: Vec2) -> f32 {
            a.x * b.y - a.y * b.x
        }

        // Original orientation from triangle F1: a→b→c (CCW)
        let orig_z = cross2d(uvb - uva, uvc - uva);

        // New triangle 1 (replaces F1): a→d→c must be CCW → cross(uvd-uva, uvc-uva) > 0
        let new_z1 = cross2d(uvd - uva, uvc - uva);

        // New triangle 2 (replaces F2): d→b→c must be CCW → cross(uvb-uvd, uvc-uvd) > 0
        let new_z2 = cross2d(uvb - uvd, uvc - uvd);

        // All must have the same sign as the original (convex quad, same orientation)
        if orig_z * new_z1 <= 0.0 || orig_z * new_z2 <= 0.0 {
            return false;
        }

        // Perform the flip
        // New cycles:
        //   F1: he4(a→d) → he0(d→c) → he2(c→a) → he4
        //   F2: he1(b→c) → he3(c→d) → he5(d→b) → he1

        // Update origins
        self.half_edges[he_id.0].origin = d; // he0: a→b becomes d→c
        self.half_edges[twin_id.0].origin = c; // he3: b→a becomes c→d

        // Update next/prev for F1 cycle: he4 → he0 → he2 → he4
        self.half_edges[he_id.0].next = he_prev;
        self.half_edges[he_id.0].prev = twin_next;
        self.half_edges[he_prev.0].next = twin_next;
        self.half_edges[he_prev.0].prev = he_id;
        self.half_edges[twin_next.0].next = he_id;
        self.half_edges[twin_next.0].prev = he_prev;

        // Update next/prev for F2 cycle: he1 → he3 → he5 → he1
        self.half_edges[twin_id.0].next = twin_prev;
        self.half_edges[twin_id.0].prev = he_next;
        self.half_edges[he_next.0].next = twin_id;
        self.half_edges[he_next.0].prev = twin_prev;
        self.half_edges[twin_prev.0].next = he_next;
        self.half_edges[twin_prev.0].prev = twin_id;

        // Update face assignments (he4 moves to F1, he1 moves to F2)
        self.half_edges[twin_next.0].face = face1;
        self.half_edges[he_next.0].face = face2;

        // Update face half_edge pointers
        self.faces[face1.0].half_edge = he_id;
        self.faces[face2.0].half_edge = twin_id;

        // Update vertex outgoing pointers
        if self.vertices[a.0].outgoing == he_id {
            self.vertices[a.0].outgoing = twin_next; // he4: a→d
        }
        if self.vertices[b.0].outgoing == twin_id {
            self.vertices[b.0].outgoing = he_next; // he1: b→c
        }

        true
    }

    /// Check if two vertices are connected by an edge.
    fn are_vertices_connected(&self, v1: VertexId, v2: VertexId) -> bool {
        let start = self.vertices[v1.0].outgoing;
        if start.0 == usize::MAX {
            return false;
        }
        let mut he = start;
        loop {
            if self.half_edges[self.half_edges[he.0].next.0].origin == v2 {
                return true;
            }
            let twin = self.half_edges[he.0].twin;
            if twin.0 == usize::MAX {
                break;
            }
            he = self.half_edges[twin.0].next;
            if he == start {
                break;
            }
        }
        false
    }

    // ── Validate ────────────────────────────────────────────────────

    pub fn validate(&self) -> bool {
        let mut ok = true;

        // Check half-edge twin consistency
        for (i, he) in self.half_edges.iter().enumerate() {
            // Boundary edges (no twin) are allowed
            if he.twin.0 == usize::MAX {
                continue;
            }
            if he.twin.0 >= self.half_edges.len() {
                log::warn!("Half-edge {} has out-of-bounds twin {:?}", i, he.twin);
                ok = false;
            } else if self.half_edges[he.twin.0].twin != HalfEdgeId(i) {
                log::warn!(
                    "Half-edge {} twin {} does not point back (points to {:?})",
                    i,
                    he.twin.0,
                    self.half_edges[he.twin.0].twin
                );
                ok = false;
            }
        }

        // Check each face's half-edge cycle
        for (fi, face) in self.faces.iter().enumerate() {
            if !face.valid {
                continue;
            }
            if face.half_edge.0 >= self.half_edges.len() {
                log::warn!("Face {} has invalid half_edge {:?}", fi, face.half_edge);
                ok = false;
                continue;
            }
            let hes = self.face_half_edges(FaceId(fi));
            for &he_id in &hes {
                if self.half_edges[he_id.0].face != FaceId(fi) {
                    log::warn!(
                        "Half-edge {:?} belongs to face {:?} but is in face {}'s cycle",
                        he_id,
                        self.half_edges[he_id.0].face,
                        fi
                    );
                    ok = false;
                }
            }
        }

        // Check vertex outgoing half-edge
        for (vi, v) in self.vertices.iter().enumerate() {
            if v.outgoing.0 != usize::MAX && self.half_edges[v.outgoing.0].origin != VertexId(vi) {
                log::warn!(
                    "Vertex {} outgoing {:?} has origin {:?}",
                    vi,
                    v.outgoing,
                    self.half_edges[v.outgoing.0].origin
                );
                ok = false;
            }
        }

        ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_triangle() {
        let positions = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ];
        let uvs = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.0, 1.0),
        ];
        let faces = vec![(0, 1, 2)];

        let mesh = HalfEdgeMesh::from_triangles(&positions, &uvs, &faces);
        assert_eq!(mesh.vertices.len(), 3);
        assert_eq!(mesh.half_edges.len(), 3);
        assert_eq!(mesh.num_valid_faces(), 1);
        assert!(mesh.validate());
    }

    #[test]
    fn test_two_triangles_shared_edge() {
        let positions = vec![
            Vec3::new(0.0, 0.0, 0.0), // v0
            Vec3::new(1.0, 0.0, 0.0), // v1
            Vec3::new(0.0, 1.0, 0.0), // v2
            Vec3::new(1.0, 1.0, 0.0), // v3
        ];
        let uvs = vec![Vec2::ZERO; 4];
        let faces = vec![(0, 1, 2), (1, 3, 2)];

        let mesh = HalfEdgeMesh::from_triangles(&positions, &uvs, &faces);
        assert_eq!(mesh.vertices.len(), 4);
        assert_eq!(mesh.half_edges.len(), 6);
        assert_eq!(mesh.num_valid_faces(), 2);
        assert!(mesh.validate());

        // Verify twins exist between the two triangles
        let mut twin_count = 0;
        for he in &mesh.half_edges {
            if he.twin.0 != usize::MAX {
                twin_count += 1;
            }
        }
        assert_eq!(twin_count, 2); // one shared edge = 2 half-edges with twins
    }

    #[test]
    fn test_edge_flip() {
        // Quad: v0(0,0), v1(1,0), v2(1,1), v3(0,1)
        // Two triangles sharing diagonal v0-v2
        let positions = vec![
            Vec3::new(0.0, 0.0, 0.0), // v0
            Vec3::new(1.0, 0.0, 0.0), // v1
            Vec3::new(1.0, 1.0, 0.0), // v2
            Vec3::new(0.0, 1.0, 0.0), // v3
        ];
        // 有效 UV（正方形域）：flip_edge 的凸性检查依赖 UV，全零 UV 会使检查恒失败
        let uvs = vec![
            Vec2::new(0.0, 0.0), // v0
            Vec2::new(1.0, 0.0), // v1
            Vec2::new(1.0, 1.0), // v2
            Vec2::new(0.0, 1.0), // v3
        ];
        let faces = vec![(0, 1, 2), (0, 2, 3)];

        let mut mesh = HalfEdgeMesh::from_triangles(&positions, &uvs, &faces);
        assert!(mesh.validate());

        // Find the shared edge (v0→v2 in F2 or v2→v0 in F1)
        let shared_he = mesh
            .half_edges
            .iter()
            .enumerate()
            .position(|(_, he)| {
                he.twin.0 != usize::MAX
                    && he.origin == VertexId(2)
                    && mesh.half_edges[he.next.0].origin == VertexId(0)
            })
            .unwrap();

        // Flip the shared edge
        let flipped = mesh.flip_edge(HalfEdgeId(shared_he));
        assert!(flipped);
        assert!(mesh.validate());

        // Flip back should also work
        let shared_he2 = mesh
            .half_edges
            .iter()
            .enumerate()
            .position(|(_, he)| {
                he.twin.0 != usize::MAX
                    && he.origin == VertexId(1)
                    && mesh.half_edges[he.next.0].origin == VertexId(3)
            })
            .unwrap();
        let flipped2 = mesh.flip_edge(HalfEdgeId(shared_he2));
        assert!(flipped2);
        assert!(mesh.validate());
    }
}
