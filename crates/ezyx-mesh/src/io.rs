//! STL and OBJ.

use crate::{MeshError, TriMesh};
use glam::Vec3;
use std::io::Write;
use std::path::Path;

pub fn load(path: &Path) -> Result<TriMesh, MeshError> {
    let bytes = std::fs::read(path)?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "obj" => read_obj(&String::from_utf8_lossy(&bytes)),
        _ => read_stl(&bytes),
    }
}

pub fn read_stl(bytes: &[u8]) -> Result<TriMesh, MeshError> {
    let is_ascii = bytes.starts_with(b"solid") && {
        // Some binary files also start with "solid": check the size relation.
        if bytes.len() >= 84 {
            let n = u32::from_le_bytes([bytes[80], bytes[81], bytes[82], bytes[83]]) as usize;
            84 + n * 50 != bytes.len()
        } else {
            true
        }
    };
    let mut m = TriMesh::default();
    if is_ascii {
        let txt = String::from_utf8_lossy(bytes);
        let mut verts = Vec::new();
        for line in txt.lines() {
            let l = line.trim();
            if let Some(r) = l.strip_prefix("vertex") {
                let v: Vec<f32> = r.split_whitespace().filter_map(|s| s.parse().ok()).collect();
                if v.len() == 3 {
                    verts.push(Vec3::new(v[0], v[1], v[2]));
                }
            }
        }
        for tri in verts.chunks_exact(3) {
            let b = m.positions.len() as u32;
            m.positions.extend_from_slice(tri);
            m.indices.push([b, b + 1, b + 2]);
        }
    } else {
        if bytes.len() < 84 {
            return Err(MeshError::Parse("STL too short".into()));
        }
        let n = u32::from_le_bytes([bytes[80], bytes[81], bytes[82], bytes[83]]) as usize;
        if bytes.len() < 84 + n * 50 {
            return Err(MeshError::Parse("STL truncated".into()));
        }
        let f = |o: usize| f32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
        for i in 0..n {
            let o = 84 + i * 50 + 12;
            let b = m.positions.len() as u32;
            for k in 0..3 {
                let q = o + k * 12;
                m.positions.push(Vec3::new(f(q), f(q + 4), f(q + 8)));
            }
            m.indices.push([b, b + 1, b + 2]);
        }
    }
    m.weld(1e-5);
    Ok(m)
}

pub fn write_stl(m: &TriMesh, path: &Path) -> Result<(), MeshError> {
    let mut out = Vec::with_capacity(84 + m.indices.len() * 50);
    let mut header = [0u8; 80];
    let tag = b"EZ-YX-craft binary STL";
    header[..tag.len()].copy_from_slice(tag);
    out.extend_from_slice(&header);
    out.extend_from_slice(&(m.indices.len() as u32).to_le_bytes());
    for i in 0..m.indices.len() {
        let [a, b, c] = m.triangle(i);
        let n = (b - a).cross(c - a).normalize_or_zero();
        for v in [n, a, b, c] {
            for k in 0..3 {
                out.extend_from_slice(&v[k].to_le_bytes());
            }
        }
        out.extend_from_slice(&[0, 0]);
    }
    std::fs::write(path, out)?;
    Ok(())
}

pub fn read_obj(txt: &str) -> Result<TriMesh, MeshError> {
    let mut m = TriMesh::default();
    for line in txt.lines() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("v") => {
                let v: Vec<f32> = it.take(3).filter_map(|s| s.parse().ok()).collect();
                if v.len() == 3 {
                    m.positions.push(Vec3::new(v[0], v[1], v[2]));
                }
            }
            Some("f") => {
                let idx: Vec<u32> = it
                    .filter_map(|s| s.split('/').next()?.parse::<i64>().ok())
                    .map(|i| if i < 0 { (m.positions.len() as i64 + i) as u32 } else { (i - 1) as u32 })
                    .collect();
                for k in 1..idx.len().saturating_sub(1) {
                    m.indices.push([idx[0], idx[k], idx[k + 1]]);
                }
            }
            _ => {}
        }
    }
    if m.indices.iter().flatten().any(|i| *i as usize >= m.positions.len()) {
        return Err(MeshError::Parse("OBJ index out of range".into()));
    }
    Ok(m)
}

pub fn write_obj(m: &TriMesh, path: &Path) -> Result<(), MeshError> {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    writeln!(f, "# EZ-YX-craft")?;
    for p in &m.positions {
        writeln!(f, "v {} {} {}", p.x, p.y, p.z)?;
    }
    for t in &m.indices {
        writeln!(f, "f {} {} {}", t[0] + 1, t[1] + 1, t[2] + 1)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stl_roundtrip() {
        let c = TriMesh::cube(Vec3::ZERO, Vec3::ONE);
        let dir = std::env::temp_dir().join(format!("ezyx_stl_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("c.stl");
        write_stl(&c, &p).unwrap();
        let r = load(&p).unwrap();
        assert_eq!(r.positions.len(), 8);
        assert_eq!(r.indices.len(), 12);
        assert!((r.volume() - 1.0).abs() < 1e-5);
        let o = dir.join("c.obj");
        write_obj(&c, &o).unwrap();
        assert_eq!(load(&o).unwrap().indices.len(), 12);
    }
}
