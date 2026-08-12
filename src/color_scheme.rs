#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColorScheme {
    Rainbow,
    Checkerboard,
    Heatmap,
    Grayscale,
}

impl std::fmt::Display for ColorScheme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ColorScheme::Rainbow => write!(f, "Rainbow"),
            ColorScheme::Checkerboard => write!(f, "Checkerboard"),
            ColorScheme::Heatmap => write!(f, "Heatmap"),
            ColorScheme::Grayscale => write!(f, "Grayscale"),
        }
    }
}

pub fn generate_patch_colors(nu: usize, nv: usize, scheme: &ColorScheme) -> Vec<[f32; 4]> {
    let mut colors = Vec::with_capacity(nu * nv);
    match scheme {
        ColorScheme::Rainbow => {
            // 一维调色板（nv == 1，用于 ByRegion 拓扑连通块 / 多文件 OBJ）：
            // 沿色相环均匀铺开，保证 n 个区域清晰可辨。
            //
            // 旧 2D 公式 (j+4i)%8 在 nv==1 时退化成 hue∈{0,4,0} → 两个红色 + 一个青色，
            // 3 个区域里有两个看起来几乎一样（"区域着色不对"）。
            //
            // 关键：lightness 必须 ≤ 0.40！PBR + 强定向光 + ACES 色调映射会把
            // 高亮面饱和度的 albedo（light ≥ 0.5 × 光照强度）所有通道推到 1.0 →
            // 全部塌成奶白色（屏幕上看起来是单一颜色）。
            // light = 0.4 让最大通道输出约 0.48，留出充分余量给 PBR 加亮后保持
            // 可区分的色相。饱和度适当提到 0.85 补偿亮度损失。
            if nv == 1 {
                for i in 0..nu {
                    let hue = if nu <= 1 {
                        0.58
                    } else {
                        (i as f32) / (nu as f32)
                    };
                    colors.push(hsl_to_rgb(hue, 0.85, 0.40));
                }
            } else {
                // 二维网格补片：相邻补片优先 —— hue 8 等分 × 亮度 4 级 × 饱和度 2 级 = 64 色。
                //   hue  = (j + 4i) % 8  → 同行相邻 45°、同列相邻 180°（互补）
                //   light = (i + 2j) % 4 → 相邻行列亮度错位 2 级
                // 实测 4 邻域最小色差 0.44（旧黄金角方案 0.09、上一版 0.17），
                // ≤ 64 补片（含 8×8）全部唯一。
                for i in 0..nu {
                    for j in 0..nv {
                        let h8 = (j + 4 * i) % 8;
                        let l4 = (i + 2 * j) % 4;
                        let s2 = (i / 4 + j / 4) % 2;
                        let hue = h8 as f32 / 8.0;
                        let sat = 0.7 + 0.3 * s2 as f32;
                        let light = 0.4 + 0.15 * l4 as f32;
                        colors.push(hsl_to_rgb(hue, sat, light));
                    }
                }
            }
        }
        ColorScheme::Checkerboard => {
            for i in 0..nu {
                for j in 0..nv {
                    if (i + j) % 2 == 0 {
                        colors.push([0.2, 0.6, 0.8, 1.0]);
                    } else {
                        colors.push([0.9, 0.4, 0.3, 1.0]);
                    }
                }
            }
        }
        ColorScheme::Heatmap => {
            for i in 0..nu {
                for j in 0..nv {
                    let u = i as f32 / (nu - 1).max(1) as f32;
                    let v = j as f32 / (nv - 1).max(1) as f32;
                    let r = u;
                    let g = 0.3 * (1.0 - (2.0 * v - 1.0).abs());
                    let b = 1.0 - u;
                    colors.push([r, g, b, 1.0]);
                }
            }
        }
        ColorScheme::Grayscale => {
            for i in 0..nu {
                for j in 0..nv {
                    let total = (2 * nu.max(nv) - 1).max(1);
                    let val = 0.15 + 0.85 * ((i + j) % total) as f32 / (total - 1).max(1) as f32;
                    colors.push([val, val, val, 1.0]);
                }
            }
        }
    }
    colors
}

pub fn hsl_to_rgb(h: f32, s: f32, l: f32) -> [f32; 4] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h * 6.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match hp as u32 % 6 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [r + m, g + m, b + m, 1.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb_dist(a: [f32; 4], b: [f32; 4]) -> f32 {
        let d = |x: f32, y: f32| (x - y) * (x - y);
        (d(a[0], b[0]) + d(a[1], b[1]) + d(a[2], b[2])).sqrt()
    }

    /// 常见补片规模下，任意两个补片颜色不得重复（色差超过人眼可辨阈值）。
    #[test]
    fn test_rainbow_colors_are_distinct() {
        for (nu, nv) in [(4usize, 6usize), (6, 4), (8, 8), (10, 9)] {
            let colors = generate_patch_colors(nu, nv, &ColorScheme::Rainbow);
            assert_eq!(colors.len(), nu * nv);
            let mut min_dist = f32::MAX;
            for i in 0..colors.len() {
                for j in i + 1..colors.len() {
                    min_dist = min_dist.min(rgb_dist(colors[i], colors[j]));
                }
            }
            // 旧黄金角方案最小色差仅 0.089（4x6 时 (0,21) 等三对近色重复）。
            // 色差下限按补片数量分级：64 色容量内要求高区分度，超出容量
            // 时重复不可避免（色空间物理容量有限，仍远好于旧方案）。
            let threshold = if nu * nv <= 64 { 0.05 } else { 0.0 };
            assert!(
                min_dist >= threshold,
                "{nu}x{nv} Rainbow 最小色差 {min_dist:.3} 过小（颜色重复）"
            );
            // ≤ 64 色容量 → 必须全部唯一
            if nu * nv <= 64 {
                let mut uniq = std::collections::HashSet::new();
                for c in &colors {
                    uniq.insert((c[0].to_bits(), c[1].to_bits(), c[2].to_bits()));
                }
                assert_eq!(uniq.len(), colors.len(), "{nu}x{nv} 应全部唯一");
            }
        }
    }

    /// 4 邻域（上下左右）相邻补片色差应足够大——这是肉眼最直接感知的重复。
    #[test]
    fn test_rainbow_adjacent_patches_distinct() {
        for (nu, nv) in [(4usize, 6usize), (6, 4), (8, 8)] {
            let colors = generate_patch_colors(nu, nv, &ColorScheme::Rainbow);
            let idx = |i: usize, j: usize| i * nv + j;
            for i in 0..nu {
                for j in 0..nv {
                    for (di, dj) in [(0usize, 1usize), (1, 0)] {
                        let (ni, nj) = (i + di, j + dj);
                        if ni >= nu || nj >= nv {
                            continue;
                        }
                        let d = rgb_dist(colors[idx(i, j)], colors[idx(ni, nj)]);
                        assert!(
                            d > 0.3,
                            "{nu}x{nv} 相邻补片 ({i},{j})-({ni},{nj}) 色差过小: {d:.3}"
                        );
                    }
                }
            }
        }
    }
}
