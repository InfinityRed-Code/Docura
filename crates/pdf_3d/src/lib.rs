//! 3D content of PDF files: the U3D (ECMA-363) model format used by
//! `/Subtype /3D` annotations.
//!
//! - [`u3d::parse`] decodes a U3D stream into a [`u3d::Scene`]
//!   (node hierarchy, meshes, shaders, materials, lights, views).
//! - [`u3d::write`] encodes triangle meshes as a U3D file that Acrobat and
//!   other 3D PDF viewers display.
//! - [`render::prepare`] turns a scene into indexed geometries, instances,
//!   materials, the node tree, cameras and lights for a WebGL renderer;
//!   [`render::pack`] serializes that into one binary buffer.
//!
//! The crate has no PDF dependency: the PDF side (finding annotations,
//! decoding the `/3DD` stream, writing the annotation) lives in
//! `pdf_mutate`.

pub mod render;
pub mod u3d;
