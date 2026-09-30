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

pub(super) use plan_my_cabinet::render::mesh::*;

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
