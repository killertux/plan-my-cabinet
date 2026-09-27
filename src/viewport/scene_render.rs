//! Scene mesh generation and renderer-owned WGPU callback resources.
use super::*;
use eframe::{egui_wgpu, wgpu};
use wgpu::util::DeviceExt;

const SCENE_SHADER: &str = r#"
struct Params {
    right: vec4<f32>, up: vec4<f32>, forward: vec4<f32>, eye: vec4<f32>,
    projection: vec4<f32>, // horizontal and vertical scale, near, far
    mode: vec4<f32>, // perspective = 1
};
@group(0) @binding(0) var<uniform> params: Params;
struct In { @location(0) position: vec3<f32>, @location(1) color: vec3<f32> };
struct Out { @builtin(position) clip: vec4<f32>, @location(0) color: vec3<f32> };
@vertex fn vs(v: In) -> Out {
    let d = v.position - params.eye.xyz;
    let depth = dot(d, params.forward.xyz);
    var o: Out;
    let w = select(1.0, depth, params.mode.x > 0.5);
    let z = select((depth - params.projection.z) / (params.projection.w - params.projection.z),
                   (depth * params.projection.w - params.projection.z * params.projection.w) /
                   (params.projection.w - params.projection.z), params.mode.x > 0.5);
    o.clip = vec4<f32>(dot(d, params.right.xyz) * params.projection.x,
                       dot(d, params.up.xyz) * params.projection.y, z, w);
    o.color = v.color;
    return o;
}
@fragment fn fs(v: Out) -> @location(0) vec4<f32> {
    return vec4<f32>(v.color, 1.0);
}
"#;

const COMPOSITE_SHADER: &str = r#"
@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var image_sampler: sampler;
struct Out { @builtin(position) clip: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs(@builtin(vertex_index) index: u32) -> Out {
    var o: Out;
    let uv = array<vec2<f32>, 3>(vec2<f32>(0.0, 0.0), vec2<f32>(2.0, 0.0), vec2<f32>(0.0, 2.0));
    o.uv = uv[index];
    o.clip = vec4<f32>(o.uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    return o;
}
@fragment fn fs(v: Out) -> @location(0) vec4<f32> {
    return textureSample(image, image_sampler, v.uv);
}
"#;

#[derive(Default)]
pub(super) struct Mesh {
    pub(super) faces: Vec<f32>,
    pub(super) shadow: Vec<f32>,
    pub(super) lines: Vec<f32>,
}

impl Mesh {
    fn vertex(out: &mut Vec<f32>, pos: [f32; 3], color: [f32; 3]) {
        out.extend(pos);
        out.extend(color);
    }

    fn line(&mut self, a: [f32; 3], b: [f32; 3], color: [f32; 3]) {
        Self::vertex(&mut self.lines, a, color);
        Self::vertex(&mut self.lines, b, color);
    }

    pub(super) fn box_mesh(&mut self, corners: [[f32; 3]; 8], color: [f32; 3], edge: [f32; 3]) {
        // `board_corners` uses bit-coded XYZ indexes (2 = min-X/max-Y,
        // 3 = max-X/max-Y). The quad topology below uses perimeter order.
        // Convert once before emitting faces and edges; otherwise each broad
        // face becomes a self-crossing bow-tie of two long triangles.
        let corners = [
            corners[0], corners[1], corners[3], corners[2], corners[4], corners[5], corners[7],
            corners[6],
        ];
        for (face, shade) in [
            ([0, 3, 2, 1], 0.55),
            ([4, 5, 6, 7], 1.0),
            ([0, 1, 5, 4], 0.75),
            ([1, 2, 6, 5], 0.85),
            ([2, 3, 7, 6], 0.68),
            ([3, 0, 4, 7], 0.8),
        ] {
            // Mix toward ambient warmth rather than multiplying sRGB channels to
            // black. This is display shading only; the saved color is unchanged.
            let shaded = std::array::from_fn(|i| color[i] * shade + 0.12 * (1.0 - shade));
            for i in [0, 1, 2, 0, 2, 3] {
                Self::vertex(&mut self.faces, corners[face[i]], shaded);
            }
        }
        for (a, b) in [
            (0, 1),
            (1, 2),
            (2, 3),
            (3, 0),
            (4, 5),
            (5, 6),
            (6, 7),
            (7, 4),
            (0, 4),
            (1, 5),
            (2, 6),
            (3, 7),
        ] {
            self.line(corners[a], corners[b], edge);
        }
    }
}

pub(super) fn highlight_color(project: &Project, id: Uuid, selection: &Selection) -> [f32; 3] {
    if selection.active == Some(id) {
        [0.79, 0.45, 0.12]
    } else if selection.ids.contains(&id) {
        [0.16, 0.59, 0.67]
    } else if selected_board(project, &selection.ids, id) {
        [0.40, 0.57, 0.59]
    } else {
        [0.55, 0.51, 0.45]
    }
}

const BACKGROUND: [f32; 3] = [236.0 / 255.0, 232.0 / 255.0, 225.0 / 255.0];
const NEUTRAL: [f32; 3] = [200.0 / 255.0, 196.0 / 255.0, 187.0 / 255.0];

fn board_face_color(project: &Project, board: &Board, material_tint: bool) -> [f32; 3] {
    if material_tint {
        project
            .material_color(board.material_id)
            .0
            .map(|channel| f32::from(channel) / 255.0)
    } else {
        NEUTRAL
    }
}

fn selection_face_color(base: [f32; 3], active: bool) -> [f32; 3] {
    if active {
        std::array::from_fn(|i| base[i] * 0.38 + [1.0, 0.70, 0.30][i] * 0.62)
    } else {
        base
    }
}

/// A softly faded grounding patch, drawn at world Z=-2 below the grid and
/// cabinet. It has no picking surface or physical/shadow-map interpretation.
fn add_floor_shadow(mesh: &mut Mesh, bounds: Bounds, camera: &Camera) {
    if !bounds.valid() {
        return;
    }
    let pad = 65.0;
    let inner = [
        bounds.min[0] - pad,
        bounds.min[1] - pad,
        bounds.max[0] + pad,
        bounds.max[1] + pad,
    ];
    let outer = [
        inner[0] - pad,
        inner[1] - pad,
        inner[2] + pad,
        inner[3] + pad,
    ];
    let ring = |r: [f64; 4]| {
        [
            [r[0], r[1], -2.0],
            [r[2], r[1], -2.0],
            [r[2], r[3], -2.0],
            [r[0], r[3], -2.0],
        ]
        .map(|p| relative(p, camera.target))
    };
    let inner = ring(inner);
    let outer = ring(outer);
    let shade = [0.78, 0.76, 0.71];
    for i in [0, 1, 2, 0, 2, 3] {
        Mesh::vertex(&mut mesh.shadow, inner[i], shade);
    }
    for i in 0..4 {
        let next = (i + 1) % 4;
        for (point, color) in [
            (outer[i], BACKGROUND),
            (inner[i], shade),
            (inner[next], shade),
            (outer[i], BACKGROUND),
            (inner[next], shade),
            (outer[next], BACKGROUND),
        ] {
            Mesh::vertex(&mut mesh.shadow, point, color);
        }
    }
}

#[cfg(test)]
pub(super) fn scene(project: &Project, camera: &Camera, selection: &Selection) -> (Mesh, f64) {
    scene_with_faces(project, camera, selection, None, None, true)
}

pub(super) fn scene_with_faces(
    project: &Project,
    camera: &Camera,
    selection: &Selection,
    faces: Option<(Uuid, BoardFace, Uuid, BoardFace)>,
    poses: Option<&HashMap<Uuid, plan_my_cabinet::units::Pose>>,
    material_tint: bool,
) -> (Mesh, f64) {
    scene_with_hover(project, camera, selection, faces, poses, material_tint, None)
}

pub(super) fn scene_with_hover(
    project: &Project,
    camera: &Camera,
    selection: &Selection,
    faces: Option<(Uuid, BoardFace, Uuid, BoardFace)>,
    poses: Option<&HashMap<Uuid, plan_my_cabinet::units::Pose>>,
    material_tint: bool,
    hovered: Option<Uuid>,
) -> (Mesh, f64) {
    let hovered: HashSet<Uuid> = hovered.into_iter().collect();
    let mut mesh = Mesh::default();
    add_grid(&mut mesh, project, camera);
    for (end, color) in [
        ([1000.0, 0.0, 0.0], [0.77, 0.27, 0.23]),
        ([0.0, 1000.0, 0.0], [0.31, 0.60, 0.34]),
        ([0.0, 0.0, 1000.0], [0.24, 0.44, 0.77]),
    ] {
        mesh.line(
            relative([0.0; 3], camera.target),
            relative(end, camera.target),
            color,
        );
    }
    let mut all = Bounds::empty();
    for board in &project.boards {
        if !selection.visible(project, board.id) {
            continue;
        }
        let world = poses
            .and_then(|p| p.get(&board.id).copied())
            .map(|pose| {
                box_corners(
                    pose,
                    board
                        .blank_dimensions()
                        .map(|d| d.micrometres() as f64 / 1000.0),
                )
            })
            .unwrap_or_else(|| board_corners(project, board));
        if let Some(world) = world {
            let corners = world.map(|p| {
                all.include(p);
                relative(p, camera.target)
            });
            let base = board_face_color(project, board, material_tint);
            // A selected broad face needs a readable warm fill as well as an
            // edge: a shelf inside the carcass otherwise blends into the same
            // white material behind it. Selection remains session-only and
            // never changes the persisted display colour or stock identity.
            let active = selection.active == Some(board.id);
            let hover = !active
                && !hovered.is_empty()
                && super::selected_board(project, &hovered, board.id);
            let face = if hover {
                std::array::from_fn(|i| base[i] * 0.78 + [1.0, 0.86, 0.66][i] * 0.22)
            } else {
                selection_face_color(base, active)
            };
            let edge = if hover && !selection.ids.contains(&board.id) {
                [0.79, 0.45, 0.12]
            } else {
                highlight_color(project, board.id, selection)
            };
            mesh.box_mesh(corners, face, edge);
            if let Some((source, source_face, target, target_face)) = faces {
                let selected = if source == board.id {
                    Some((source_face, [0.15, 0.95, 0.95]))
                } else if target == board.id {
                    Some((target_face, [1.0, 0.25, 0.8]))
                } else {
                    None
                };
                if let Some((face, color)) = selected {
                    let fixed = 1 << face.axis;
                    let side = face.side == Side::Positive;
                    let indexes: Vec<_> = (0..8).filter(|i| (*i & fixed != 0) == side).collect();
                    let loop_indices = [indexes[0], indexes[1], indexes[3], indexes[2]];
                    for i in 0..4 {
                        mesh.line(
                            corners[loop_indices[i]],
                            corners[loop_indices[(i + 1) % 4]],
                            color,
                        );
                    }
                    for a in [loop_indices[0], loop_indices[1]] {
                        mesh.line(corners[a], corners[loop_indices[2]], color);
                    }
                }
            }
        }
    }
    for hardware in &project.hardware {
        let HardwareKind::Placeholder { dimensions } = hardware.kind else {
            continue;
        };
        if !selection.visible(project, hardware.id) {
            continue;
        }
        if let Some(pose) = poses
            .and_then(|p| p.get(&hardware.id).copied())
            .or_else(|| plan_my_cabinet::assembly_edit::world_pose(project, hardware.id).ok())
            && let Some(world) =
                box_corners(pose, dimensions.map(|d| d.micrometres() as f64 / 1000.0))
        {
            let corners = world.map(|p| {
                all.include(p);
                relative(p, camera.target)
            });
            mesh.box_mesh(
                corners,
                [0.66, 0.70, 0.69],
                highlight_color(project, hardware.id, selection),
            );
        }
    }
    add_floor_shadow(&mut mesh, all, camera);
    let radius = if all.valid() {
        (0..3)
            .map(|i| {
                (all.min[i] - camera.target[i])
                    .abs()
                    .max((all.max[i] - camera.target[i]).abs())
                    .powi(2)
            })
            .sum::<f64>()
            .sqrt()
    } else {
        0.0
    };
    (mesh, radius.max(4000.0))
}

/// Draw lines on exact multiples of the project grid. At small spacings use
/// integer multiples of the spacing to keep line count and pixel density sane.
pub(super) fn grid_display_interval(project: &Project, camera: &Camera) -> f64 {
    let spacing = project.grid_spacing.micrometres() as f64 / 1000.0;
    let visible_radius = (camera.distance * (FOV / 2.0).tan() * 2.5).clamp(100.0, 2_000_000.0);
    let desired = (visible_radius / 24.0).max(spacing);
    let multiple = 10_f64.powf((desired / spacing).log10().ceil().max(0.0));
    spacing * multiple
}

pub(super) fn add_grid(mesh: &mut Mesh, project: &Project, camera: &Camera) {
    let visible_radius = (camera.distance * (FOV / 2.0).tan() * 2.5).clamp(100.0, 2_000_000.0);
    let half = visible_radius.max(1000.0);
    let step = grid_display_interval(project, camera);
    let center = camera.target;
    let range = |axis: usize| {
        let low = ((center[axis] - half) / step).ceil() as i64;
        let high = ((center[axis] + half) / step).floor() as i64;
        low..=high.min(low + 100)
    };
    for axis in 0..2 {
        for i in range(axis) {
            let n = i as f64 * step;
            let mut a = [center[0] - half, center[1] - half, 0.0];
            let mut b = [center[0] + half, center[1] + half, 0.0];
            a[axis] = n;
            b[axis] = n;
            // World axes remain visible even when a coarse LOD skips fine lines.
            let color = if i == 0 {
                [0.65, 0.59, 0.51]
            } else if i.rem_euclid(10) == 0 {
                [0.81, 0.78, 0.73]
            } else {
                [0.87, 0.84, 0.80]
            };
            mesh.line(relative(a, center), relative(b, center), color);
        }
    }
}

fn relative(point: [f64; 3], origin: [f64; 3]) -> [f32; 3] {
    std::array::from_fn(|i| (point[i] - origin[i]) as f32)
}

fn bytes(data: &[f32]) -> Vec<u8> {
    data.iter().flat_map(|v| v.to_ne_bytes()).collect()
}

struct Targets {
    size: [u32; 2],
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
    bind: wgpu::BindGroup,
}

struct Resources {
    faces: wgpu::Buffer,
    face_count: u32,
    shadow: wgpu::Buffer,
    shadow_count: u32,
    lines: wgpu::Buffer,
    line_count: u32,
    params: wgpu::Buffer,
    params_bind: wgpu::BindGroup,
    faces_pipeline: wgpu::RenderPipeline,
    lines_pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,
    texture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    format: wgpu::TextureFormat,
    targets: Option<Targets>,
}

fn scene_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    topology: wgpu::PrimitiveTopology,
    depth_write: bool,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("viewport scene"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: 24,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
            })],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(format.into())],
        }),
        primitive: wgpu::PrimitiveState {
            topology,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth24Plus,
            depth_write_enabled: Some(depth_write),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: Default::default(),
            bias: wgpu::DepthBiasState {
                constant: if depth_write { 1 } else { 0 },
                ..Default::default()
            },
        }),
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    })
}

impl Resources {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let empty = [0_u8; 24];
        let faces = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("box faces"),
            contents: &empty,
            usage: wgpu::BufferUsages::VERTEX,
        });
        let shadow = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("floor shadow"),
            contents: &empty,
            usage: wgpu::BufferUsages::VERTEX,
        });
        let lines = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("outlines, axes and grid"),
            contents: &empty,
            usage: wgpu::BufferUsages::VERTEX,
        });
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewport camera"),
            size: 96,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let params_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("viewport parameters"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let params_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &params_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: params.as_entire_binding(),
            }],
        });
        let scene_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&params_layout)],
            immediate_size: 0,
        });
        let scene_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene shader"),
            source: wgpu::ShaderSource::Wgsl(SCENE_SHADER.into()),
        });
        let faces_pipeline = scene_pipeline(
            device,
            &scene_shader,
            &scene_layout,
            format,
            wgpu::PrimitiveTopology::TriangleList,
            true,
        );
        let lines_pipeline = scene_pipeline(
            device,
            &scene_shader,
            &scene_layout,
            format,
            wgpu::PrimitiveTopology::LineList,
            false,
        );
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("viewport image"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let composite_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&texture_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("viewport composite shader"),
            source: wgpu::ShaderSource::Wgsl(COMPOSITE_SHADER.into()),
        });
        let composite_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("viewport composite"),
            layout: Some(&composite_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                targets: &[Some(format.into())],
                compilation_options: Default::default(),
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        Self {
            faces,
            face_count: 0,
            shadow,
            shadow_count: 0,
            lines,
            line_count: 0,
            params,
            params_bind,
            faces_pipeline,
            lines_pipeline,
            composite_pipeline,
            texture_layout,
            sampler,
            format,
            targets: None,
        }
    }

    fn resize(&mut self, device: &wgpu::Device, size: [u32; 2]) {
        let size = bounded_target_size(size, device.limits().max_texture_dimension_2d);
        if self.targets.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let texture = |label, format, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let color = texture(
            "viewport color",
            self.format,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        )
        .create_view(&Default::default());
        let depth = texture(
            "viewport depth",
            wgpu::TextureFormat::Depth24Plus,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        )
        .create_view(&Default::default());
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("viewport image binding"),
            layout: &self.texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&color),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        self.targets = Some(Targets {
            size,
            color,
            depth,
            bind,
        });
    }
}

/// Preserve the aspect ratio while preventing invalid texture allocations even
/// if a parent layout transiently expands beyond the visible window.
fn bounded_target_size(size: [u32; 2], limit: u32) -> [u32; 2] {
    let limit = limit.max(1);
    let size = size.map(|n| n.max(1));
    let largest = size[0].max(size[1]);
    if largest <= limit {
        return size;
    }
    size.map(|n| ((u64::from(n) * u64::from(limit)) / u64::from(largest)).max(1) as u32)
}

#[test]
fn viewport_texture_dimensions_are_bounded_without_aspect_distortion() {
    assert_eq!(bounded_target_size([2880, 1800], 8192), [2880, 1800]);
    assert_eq!(bounded_target_size([20000, 10000], 8192), [8192, 4096]);
    assert_eq!(bounded_target_size([0, 0], 8192), [1, 1]);
    assert_eq!(
        bounded_target_size([u32::MAX, u32::MAX], 8192),
        [8192, 8192]
    );
}

pub fn install(state: &egui_wgpu::RenderState) {
    state
        .renderer
        .write()
        .callback_resources
        .insert(Resources::new(&state.device, state.target_format));
}

pub(super) struct ViewportCallback {
    pub(super) size: [u32; 2],
    pub(super) mesh: Mesh,
    pub(super) uniform: Vec<u8>,
}

pub(super) fn highlighted_faces(
    tool: &MoveTool,
    placement: Option<(Uuid, BoardFace, Uuid, BoardFace)>,
) -> Option<(Uuid, BoardFace, Uuid, BoardFace)> {
    match tool.drag.as_ref() {
        Some(drag) => match drag.snap {
            Some(DragSnap::Face(snap)) => Some((
                drag.board_id,
                snap.source_face,
                snap.target_id,
                snap.target_face,
            )),
            // A live grid snap or free drag must not display a stale placement
            // face pair. Face candidates already outrank grid during snapping.
            Some(DragSnap::Grid) | None => None,
        },
        None => placement,
    }
}

#[test]
fn active_face_has_warm_fill_without_recolouring_unselected_material() {
    let pale = [0.9, 0.9, 0.9];
    assert_eq!(selection_face_color(pale, false), pale);
    let warm = selection_face_color(pale, true);
    assert!(warm[0] > warm[1] && warm[1] > warm[2]);
    assert!(warm[0] > pale[0] && warm[2] < pale[2]);
}

#[test]
fn live_snap_highlight_overrides_placement_and_grid_clears_stale_faces() {
    let pose =
        plan_my_cabinet::units::Pose::new([0.0; 3], plan_my_cabinet::units::Quaternion::IDENTITY)
            .unwrap();
    let source = Uuid::new_v4();
    let target = Uuid::new_v4();
    let other = Uuid::new_v4();
    let face = BoardFace {
        axis: 0,
        side: Side::Positive,
    };
    let opposite = BoardFace {
        axis: 0,
        side: Side::Negative,
    };
    let placement = Some((other, face, target, opposite));
    let mut tool = MoveTool::default();
    assert_eq!(highlighted_faces(&tool, placement), placement);
    tool.drag = Some(MoveDrag {
        board_id: source,
        start: egui::Pos2::ZERO,
        world: pose,
        selection_ids: Default::default(),
        selection_active: Some(source),
        snap: Some(DragSnap::Grid),
        last_pose: None,
    });
    assert_eq!(highlighted_faces(&tool, placement), None);
    tool.drag.as_mut().unwrap().snap =
        Some(DragSnap::Face(plan_my_cabinet::placement::SnapCandidate {
            target_id: target,
            target_face: opposite,
            source_face: face,
            distance_mm: 0.0,
            world_pose: pose,
        }));
    assert_eq!(
        highlighted_faces(&tool, placement),
        Some((source, face, target, opposite))
    );
    tool.drag.as_mut().unwrap().snap = None;
    assert_eq!(highlighted_faces(&tool, placement), None);
}

impl egui_wgpu::CallbackTrait for ViewportCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(r) = resources.get_mut::<Resources>() else {
            return Vec::new();
        };
        r.resize(device, self.size);
        queue.write_buffer(&r.params, 0, &self.uniform);
        if !self.mesh.faces.is_empty() {
            r.faces = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("board faces"),
                contents: &bytes(&self.mesh.faces),
                usage: wgpu::BufferUsages::VERTEX,
            });
        }
        r.face_count = (self.mesh.faces.len() / 6) as u32;
        if !self.mesh.shadow.is_empty() {
            r.shadow = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("floor shadow"),
                contents: &bytes(&self.mesh.shadow),
                usage: wgpu::BufferUsages::VERTEX,
            });
        }
        r.shadow_count = (self.mesh.shadow.len() / 6) as u32;
        r.lines = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("board edges and grid"),
            contents: &bytes(&self.mesh.lines),
            usage: wgpu::BufferUsages::VERTEX,
        });
        r.line_count = (self.mesh.lines.len() / 6) as u32;
        let target = r.targets.as_ref().expect("viewport target allocated");
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("3D viewport"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target.color,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: BACKGROUND[0] as f64,
                        g: BACKGROUND[1] as f64,
                        b: BACKGROUND[2] as f64,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &target.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, &r.params_bind, &[]);
        pass.set_pipeline(&r.faces_pipeline);
        pass.set_vertex_buffer(0, r.shadow.slice(..));
        pass.draw(0..r.shadow_count, 0..1);
        pass.set_vertex_buffer(0, r.faces.slice(..));
        pass.draw(0..r.face_count, 0..1);
        pass.set_pipeline(&r.lines_pipeline);
        pass.set_vertex_buffer(0, r.lines.slice(..));
        pass.draw(0..r.line_count, 0..1);
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::epaint::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        if let Some(r) = resources.get::<Resources>()
            && let Some(target) = &r.targets
        {
            pass.set_pipeline(&r.composite_pipeline);
            pass.set_bind_group(0, &target.bind, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn paint(
    ui: &egui::Ui,
    project: &Project,
    camera: &Camera,
    selection: &Selection,
    tool: &MoveTool,
    faces: Option<(Uuid, BoardFace, Uuid, BoardFace)>,
    poses: Option<&HashMap<Uuid, plan_my_cabinet::units::Pose>>,
    material_tint: bool,
    rect: egui::Rect,
) {
    let pixels = ui.ctx().pixels_per_point();
    let size = [
        (rect.width() * pixels).round().max(1.0) as u32,
        (rect.height() * pixels).round().max(1.0) as u32,
    ];
    let faces = highlighted_faces(tool, faces);
    let capture_pose = tool.capture_snap.and(tool.drag.as_ref()).and_then(|drag| {
        drag.last_pose
            .map(|pose| HashMap::from([(drag.board_id, pose)]))
    });
    let hover = if tool.drag.is_some() {
        None
    } else {
        super::hovered(ui.ctx())
    };
    let (mesh, radius) = scene_with_hover(
        project,
        camera,
        selection,
        faces,
        capture_pose.as_ref().or(poses),
        material_tint,
        hover,
    );
    ui.painter().add(egui_wgpu::Callback::new_paint_callback(
        rect,
        ViewportCallback {
            size,
            mesh,
            uniform: camera.uniform(size, radius),
        },
    ));
}
