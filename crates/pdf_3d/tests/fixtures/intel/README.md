# U3D test scenes

These `.u3d` files are unmodified copies of test scenes from the
**Universal 3D Sample Software** (Intel Corporation and contributors),
`Samples/TestScenes/`, as mirrored at <https://github.com/ningfei/u3d>.

They are licensed under the Apache License, Version 2.0
(<http://www.apache.org/licenses/LICENSE-2.0>). They are used only as test
input for the U3D decoder.

| File | Content |
|------|---------|
| `box_base.u3d` | Box, base mesh only (compressed face indices) |
| `surface_base.u3d` | Surface, base mesh only |
| `shading2_base.u3d` | Two shaders on one mesh, base mesh only |
| `plane64x64_base.u3d` | Textured 64x64 plane, base mesh only |
| `box.u3d` | Box as a progressive mesh (empty base mesh) — twin of `box_base.u3d` |
| `surface.u3d` | Progressive twin of `surface_base.u3d` |
| `plane64x64.u3d` | Progressive twin of `plane64x64_base.u3d` |
| `VertexColor.u3d` | Progressive meshes with per-vertex colors (teapot) |
| `TextureLayers.u3d` | Progressive mesh with texture layers |

The progressive/base twins are the oracle for the progressive mesh decoder:
both must decode to the same geometry.
