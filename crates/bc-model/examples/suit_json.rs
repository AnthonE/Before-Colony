//! Writes a frame's model as JSON, for looking at it outside the game (`scripts/suit-render.py`
//! renders it in Blender): every bone's mesh at its level of detail (empty where it has none), in
//! the bone's own space, with its joint and parent, and the sockets.
//!
//! `cargo run -p bc-model --example suit_json -- leo [far] [head=wingzero arms=taurus ...] > leo.json`
//!
//! Each `section=frame` swaps that section ([`bc_model::Section`]: head, body, arms, legs, backpack,
//! weapon) for another frame's.

use std::fmt::Write as _;

use bc_model::rig::{ALL, Bone};
use bc_model::{Lod, Parts, Section, build_parts};
use bc_proto::FrameId;

fn frame_named(name: &str) -> FrameId {
    let name = name.to_lowercase();
    FrameId::ALL.into_iter().find(|f| format!("{f:?}").to_lowercase() == name).unwrap_or_else(|| {
        let names: Vec<String> = FrameId::ALL.iter().map(|f| format!("{f:?}").to_lowercase()).collect();
        eprintln!("no frame {name}: one of {}", names.join(", "));
        std::process::exit(2);
    })
}

fn floats(out: &mut String, xs: impl IntoIterator<Item = f32>) {
    out.push('[');
    for (i, x) in xs.into_iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "{}", (x * 10_000.0).round() / 10_000.0);
    }
    out.push(']');
}

fn main() {
    let mut args = std::env::args().skip(1);
    let frame = frame_named(&args.next().unwrap_or_else(|| "leo".into()));
    let (mut lod, mut parts) = (Lod::Near, Parts::whole(frame));
    for arg in args {
        if arg == "far" {
            lod = Lod::Far;
            continue;
        }
        let Some((section, other)) = arg.split_once('=') else {
            eprintln!("{arg}: want far, or section=frame");
            std::process::exit(2);
        };
        let Some(section) = Section::ALL.into_iter().find(|s| format!("{s:?}").eq_ignore_ascii_case(section))
        else {
            eprintln!("no section {section}: one of {:?}", Section::ALL);
            std::process::exit(2);
        };
        parts = parts.with(section, frame_named(other));
    }
    let model = build_parts(parts, lod);
    let mut out = String::new();
    let _ = write!(out, "{{\"frame\":\"{frame:?}\",\"triangles\":{},\"bones\":[", model.triangles());
    let mut first = true;
    let empty = bc_model::kit::MeshData::default();
    for bone in ALL {
        // Every bone, so a pose can turn a bone whose children have meshes though it has none.
        let mesh = model.bones[bone.index()].as_ref().unwrap_or(&empty);
        if !first {
            out.push(',');
        }
        first = false;
        let def = bone.def();
        let parent = def.parent.map_or("null".to_string(), |p: Bone| format!("\"{p:?}\""));
        let _ = write!(out, "{{\"name\":\"{bone:?}\",\"parent\":{parent},\"joint\":");
        floats(&mut out, def.joint.to_array());
        out.push_str(",\"positions\":");
        floats(&mut out, mesh.positions.iter().flatten().copied());
        out.push_str(",\"normals\":");
        floats(&mut out, mesh.normals.iter().flatten().copied());
        out.push_str(",\"colors\":");
        floats(&mut out, mesh.colors.iter().flatten().copied());
        out.push_str(",\"indices\":[");
        for (i, k) in mesh.indices.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            let _ = write!(out, "{k}");
        }
        out.push_str("]}");
    }
    let s = &model.sockets;
    let _ = write!(out, "],\"sockets\":{{\"eye\":");
    floats(&mut out, (s.eye + Bone::Head.def().joint).to_array());
    out.push_str(",\"muzzle\":");
    floats(&mut out, (s.muzzle + Bone::Weapon.def().joint).to_array());
    out.push_str("}}");
    println!("{out}");
}
